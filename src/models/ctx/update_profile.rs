use crate::constants::{CINEMETA_COMMUNITY_ADDONS_URL, OFFICIAL_ADDONS, PROFILE_STORAGE_KEY};
use crate::models::ctx::{CommunityAddonsResp, CtxError, CtxStatus, OtherError};
use crate::runtime::msg::{Action, ActionCtx, Event, Internal, Msg};
use crate::runtime::{Effect, EffectFuture, Effects, Env, EnvFutureExt};
use crate::types::addon::Descriptor;
use crate::types::api::{
    fetch_api, APIError, APIRequest, APIResult, CollectionResponse, SuccessResponse,
};
use crate::types::profile::{Auth, AuthKey, Profile, Settings, User};
use crate::types::OptionInspectExt;
use enclose::enclose;
use futures::{future, FutureExt, TryFutureExt};
use http::request::Request;

pub fn update_profile<E: Env + 'static>(
    profile: &mut Profile,
    status: &CtxStatus,
    msg: &Msg,
) -> Effects {
    match msg {
        Msg::Action(Action::Ctx(ActionCtx::Logout)) | Msg::Internal(Internal::Logout) => {
            let next_profile = Profile::default();
            if *profile != next_profile {
                *profile = next_profile;
                Effects::msg(Msg::Internal(Internal::ProfileChanged))
            } else {
                Effects::none().unchanged()
            }
        }
        Msg::Action(Action::Ctx(ActionCtx::PushUserToAPI)) => match &profile.auth {
            Some(Auth { key, user }) => {
                Effects::one(push_user_to_api::<E>(user.to_owned(), key)).unchanged()
            }
            _ => Effects::msg(Msg::Event(Event::Error {
                error: CtxError::from(OtherError::UserNotLoggedIn),
                source: Box::new(Event::UserPushedToAPI { uid: profile.uid() }),
            }))
            .unchanged(),
        },
        Msg::Action(Action::Ctx(ActionCtx::PullUserFromAPI)) => match profile.auth_key() {
            Some(auth_key) => Effects::one(pull_user_from_api::<E>(auth_key)).unchanged(),
            _ => Effects::msg(Msg::Event(Event::Error {
                error: CtxError::from(OtherError::UserNotLoggedIn),
                source: Box::new(Event::UserPulledFromAPI { uid: profile.uid() }),
            }))
            .unchanged(),
        },
        Msg::Action(Action::Ctx(ActionCtx::PushAddonsToAPI)) => match profile.auth_key() {
            Some(auth_key) => {
                Effects::one(push_addons_to_api::<E>(profile.addons.to_owned(), auth_key))
                    .unchanged()
            }
            _ => Effects::msg(Msg::Event(Event::Error {
                error: CtxError::from(OtherError::UserNotLoggedIn),
                source: Box::new(Event::AddonsPushedToAPI {
                    transport_urls: profile
                        .addons
                        .iter()
                        .map(|addon| &addon.transport_url)
                        .cloned()
                        .collect(),
                }),
            }))
            .unchanged(),
        },
        Msg::Action(Action::Ctx(ActionCtx::PullAddonsFromAPI)) => match profile.auth_key() {
            Some(auth_key) => Effects::one(pull_addons_from_api::<E>(auth_key)).unchanged(),
            _ => Effects::one(pull_community_addons::<E>()).unchanged(),
        },
        Msg::Action(Action::Ctx(ActionCtx::InstallAddon(addon))) => {
            Effects::msg(Msg::Internal(Internal::InstallAddon(addon.to_owned()))).unchanged()
        }
        Msg::Action(Action::Ctx(ActionCtx::UpgradeAddon(addon))) => {
            if profile.addons.contains(addon) {
                return addon_upgrade_error_effects(addon, OtherError::AddonAlreadyInstalled);
            }
            if addon.manifest.behavior_hints.configuration_required {
                return addon_upgrade_error_effects(addon, OtherError::AddonConfigurationRequired);
            }
            let addon_position = match profile
                .addons
                .iter()
                .map(|addon| &addon.transport_url)
                .position(|transport_url| *transport_url == addon.transport_url)
            {
                Some(addon_position) => addon_position,
                None => return addon_upgrade_error_effects(addon, OtherError::AddonNotInstalled),
            };
            if addon.flags.protected || profile.addons[addon_position].flags.protected {
                return addon_upgrade_error_effects(addon, OtherError::AddonIsProtected);
            }
            profile.addons[addon_position] = addon.to_owned();
            let push_to_api_effects = match profile.auth_key() {
                Some(auth_key) => {
                    Effects::one(push_addons_to_api::<E>(profile.addons.to_owned(), auth_key))
                        .unchanged()
                }
                _ => Effects::none().unchanged(),
            };
            Effects::msg(Msg::Event(Event::AddonUpgraded {
                transport_url: addon.transport_url.to_owned(),
                id: addon.manifest.id.to_owned(),
            }))
            .join(push_to_api_effects)
            .join(Effects::msg(Msg::Internal(Internal::ProfileChanged)))
        }
        Msg::Action(Action::Ctx(ActionCtx::UninstallAddon(addon))) => {
            let addon_position = profile
                .addons
                .iter()
                .map(|addon| &addon.transport_url)
                .position(|transport_url| *transport_url == addon.transport_url);
            if let Some(addon_position) = addon_position {
                if !profile.addons[addon_position].flags.protected && !addon.flags.protected {
                    profile.addons.remove(addon_position);
                    let push_to_api_effects = match profile.auth_key() {
                        Some(auth_key) => Effects::one(push_addons_to_api::<E>(
                            profile.addons.to_owned(),
                            auth_key,
                        ))
                        .unchanged(),
                        _ => Effects::none().unchanged(),
                    };
                    Effects::msg(Msg::Event(Event::AddonUninstalled {
                        transport_url: addon.transport_url.to_owned(),
                        id: addon.manifest.id.to_owned(),
                    }))
                    .join(push_to_api_effects)
                    .join(Effects::msg(Msg::Internal(Internal::ProfileChanged)))
                } else {
                    addon_uninstall_error_effects(addon, OtherError::AddonIsProtected)
                }
            } else {
                addon_uninstall_error_effects(addon, OtherError::AddonNotInstalled)
            }
        }
        Msg::Action(Action::Ctx(ActionCtx::LogoutTrakt)) => match &mut profile.auth {
            Some(Auth { user, key }) => {
                if user.trakt.is_some() {
                    user.trakt = None;
                    let push_to_api_effects =
                        Effects::one(push_user_to_api::<E>(user.to_owned(), key));
                    Effects::msg(Msg::Event(Event::TraktLoggedOut { uid: profile.uid() }))
                        .join(push_to_api_effects)
                        .join(Effects::msg(Msg::Internal(Internal::ProfileChanged)))
                } else {
                    Effects::msg(Msg::Event(Event::TraktLoggedOut { uid: profile.uid() }))
                        .unchanged()
                }
            }
            _ => Effects::msg(Msg::Event(Event::Error {
                error: CtxError::from(OtherError::UserNotLoggedIn),
                source: Box::new(Event::TraktLoggedOut { uid: profile.uid() }),
            }))
            .unchanged(),
        },
        Msg::Action(Action::Ctx(ActionCtx::UpdateSettings(settings))) => {
            if profile.settings != *settings {
                profile.settings = settings.to_owned();
                Effects::msg(Msg::Event(Event::SettingsUpdated {
                    settings: settings.to_owned(),
                }))
                .join(Effects::msg(Msg::Internal(Internal::ProfileChanged)))
            } else {
                Effects::msg(Msg::Event(Event::SettingsUpdated {
                    settings: settings.to_owned(),
                }))
                .unchanged()
            }
        }
        Msg::Internal(Internal::ProfileChanged) => {
            Effects::one(push_profile_to_storage::<E>(profile)).unchanged()
        }
        Msg::Internal(Internal::InstallAddon(addon)) => {
            if !profile.addons.contains(addon) {
                if !addon.manifest.behavior_hints.configuration_required {
                    let addon_position = profile
                        .addons
                        .iter()
                        .map(|addon| &addon.transport_url)
                        .position(|transport_url| *transport_url == addon.transport_url);
                    if let Some(addon_position) = addon_position {
                        profile.addons[addon_position] = addon.to_owned();
                    } else {
                        profile.addons.push(addon.to_owned());
                    };
                    let push_to_api_effects = match profile.auth_key() {
                        Some(auth_key) => Effects::one(push_addons_to_api::<E>(
                            profile.addons.to_owned(),
                            auth_key,
                        ))
                        .unchanged(),
                        _ => Effects::none().unchanged(),
                    };
                    Effects::msg(Msg::Event(Event::AddonInstalled {
                        transport_url: addon.transport_url.to_owned(),
                        id: addon.manifest.id.to_owned(),
                    }))
                    .join(push_to_api_effects)
                    .join(Effects::msg(Msg::Internal(Internal::ProfileChanged)))
                } else {
                    addon_install_error_effects(addon, OtherError::AddonConfigurationRequired)
                }
            } else {
                addon_install_error_effects(addon, OtherError::AddonAlreadyInstalled)
            }
        }
        Msg::Internal(Internal::CtxAuthResult(auth_request, result)) => match (status, result) {
            (CtxStatus::Loading(loading_auth_request), Ok((auth, addons, _)))
                if loading_auth_request == auth_request =>
            {
                let next_profile = Profile {
                    auth: Some(auth.to_owned()),
                    addons: addons.to_owned(),
                    settings: Settings::default(),
                };
                if *profile != next_profile {
                    *profile = next_profile;
                    Effects::msg(Msg::Internal(Internal::ProfileChanged))
                } else {
                    Effects::none().unchanged()
                }
            }
            _ => Effects::none().unchanged(),
        },
        Msg::Internal(Internal::AddonsCommunityResult(result)) => {
            let mut transport_urls = vec![];
            let next_addons = match result {
                Ok(community_addons) => profile
                    .addons
                    .iter()
                    .map(|profile_addon| {
                        community_addons
                            .iter()
                            .find(|community_addon| {
                                community_addon.transport_url == profile_addon.transport_url
                                    && community_addon.manifest.version
                                        > profile_addon.manifest.version
                            })
                            .inspect_some(|community_addon| {
                                transport_urls.push(community_addon.transport_url.to_owned())
                            })
                            .map(|community_addon| Descriptor {
                                transport_url: community_addon.transport_url.to_owned(),
                                manifest: community_addon.manifest.to_owned(),
                                flags: profile_addon.flags.to_owned(),
                            })
                            .unwrap_or_else(|| profile_addon.to_owned())
                    })
                    .collect::<Vec<_>>(),
                _ => profile.addons.to_owned(),
            };
            let next_addons = next_addons
                .iter()
                .map(|profile_addon| {
                    OFFICIAL_ADDONS
                        .iter()
                        .find(|official_addon| {
                            official_addon.manifest.id == profile_addon.manifest.id
                                && official_addon.manifest.version > profile_addon.manifest.version
                        })
                        .inspect_some(|official_addon| {
                            transport_urls.push(official_addon.transport_url.to_owned())
                        })
                        .map(|official_addon| Descriptor {
                            transport_url: official_addon.transport_url.to_owned(),
                            manifest: official_addon.manifest.to_owned(),
                            flags: profile_addon.flags.to_owned(),
                        })
                        .unwrap_or_else(|| profile_addon.to_owned())
                })
                .collect::<Vec<_>>();
            if profile.addons != next_addons {
                profile.addons = next_addons;
                Effects::msg(Msg::Event(Event::AddonsPulledFromAPI { transport_urls }))
                    .join(Effects::msg(Msg::Internal(Internal::ProfileChanged)))
            } else {
                Effects::msg(Msg::Event(Event::AddonsPulledFromAPI { transport_urls })).unchanged()
            }
        }
        Msg::Internal(Internal::AddonsAPIResult(
            APIRequest::AddonCollectionGet { auth_key, .. },
            result,
        )) if profile.auth_key() == Some(auth_key) => match result {
            Ok(addons) => {
                let transport_urls = addons
                    .iter()
                    .map(|addon| &addon.transport_url)
                    .cloned()
                    .collect();
                if profile.addons != *addons {
                    profile.addons = addons.to_owned();
                    Effects::msg(Msg::Event(Event::AddonsPulledFromAPI { transport_urls }))
                        .join(Effects::msg(Msg::Internal(Internal::ProfileChanged)))
                } else {
                    Effects::msg(Msg::Event(Event::AddonsPulledFromAPI { transport_urls }))
                        .unchanged()
                }
            }
            Err(error) => Effects::msg(Msg::Event(Event::Error {
                error: error.to_owned(),
                source: Box::new(Event::AddonsPulledFromAPI {
                    transport_urls: Default::default(),
                }),
            }))
            .unchanged(),
        },
        Msg::Internal(Internal::UserAPIResult(APIRequest::GetUser { auth_key }, result))
            if profile.auth_key() == Some(auth_key) =>
        {
            match result {
                Ok(user) => match &mut profile.auth {
                    Some(auth) if auth.user != *user => {
                        auth.user = user.to_owned();
                        Effects::msg(Msg::Event(Event::UserPulledFromAPI { uid: profile.uid() }))
                            .join(Effects::msg(Msg::Internal(Internal::ProfileChanged)))
                    }
                    _ => Effects::msg(Msg::Event(Event::UserPulledFromAPI { uid: profile.uid() }))
                        .unchanged(),
                },
                Err(error) => {
                    let session_expired_effects = match error {
                        CtxError::API(APIError { code, .. }) if *code == 1 => {
                            Effects::msg(Msg::Internal(Internal::Logout)).unchanged()
                        }
                        _ => Effects::none().unchanged(),
                    };
                    Effects::msg(Msg::Event(Event::Error {
                        error: error.to_owned(),
                        source: Box::new(Event::UserPulledFromAPI { uid: profile.uid() }),
                    }))
                    .unchanged()
                    .join(session_expired_effects)
                }
            }
        }
        _ => Effects::none().unchanged(),
    }
}

fn push_addons_to_api<E: Env + 'static>(addons: Vec<Descriptor>, auth_key: &AuthKey) -> Effect {
    let transport_urls = addons
        .iter()
        .map(|addon| &addon.transport_url)
        .cloned()
        .collect();
    let request = APIRequest::AddonCollectionSet {
        auth_key: auth_key.to_owned(),
        addons,
    };
    EffectFuture::Concurrent(
        fetch_api::<E, _, _, SuccessResponse>(&request)
            .map_err(CtxError::from)
            .and_then(|result| match result {
                APIResult::Ok { result } => future::ok(result),
                APIResult::Err { error } => future::err(CtxError::from(error)),
            })
            .map(move |result| match result {
                Ok(_) => Msg::Event(Event::AddonsPushedToAPI { transport_urls }),
                Err(error) => Msg::Event(Event::Error {
                    error,
                    source: Box::new(Event::AddonsPushedToAPI { transport_urls }),
                }),
            })
            .boxed_env(),
    )
    .into()
}

fn pull_user_from_api<E: Env + 'static>(auth_key: &AuthKey) -> Effect {
    let request = APIRequest::GetUser {
        auth_key: auth_key.to_owned(),
    };
    EffectFuture::Concurrent(
        fetch_api::<E, _, _, _>(&request)
            .map_err(CtxError::from)
            .and_then(|result| match result {
                APIResult::Ok { result } => future::ok(result),
                APIResult::Err { error } => future::err(CtxError::from(error)),
            })
            .map(move |result| Msg::Internal(Internal::UserAPIResult(request, result)))
            .boxed_env(),
    )
    .into()
}

fn push_user_to_api<E: Env + 'static>(user: User, auth_key: &AuthKey) -> Effect {
    let uid = Some(user.id.to_owned());
    let request = APIRequest::SaveUser {
        auth_key: auth_key.to_owned(),
        user,
    };
    EffectFuture::Concurrent(
        fetch_api::<E, _, _, SuccessResponse>(&request)
            .map_err(CtxError::from)
            .and_then(|result| match result {
                APIResult::Ok { result } => future::ok(result),
                APIResult::Err { error } => future::err(CtxError::from(error)),
            })
            .map(move |result| match result {
                Ok(_) => Msg::Event(Event::UserPushedToAPI { uid }),
                Err(error) => Msg::Event(Event::Error {
                    error,
                    source: Box::new(Event::UserPushedToAPI { uid }),
                }),
            })
            .boxed_env(),
    )
    .into()
}

fn pull_addons_from_api<E: Env + 'static>(auth_key: &AuthKey) -> Effect {
    let request = APIRequest::AddonCollectionGet {
        auth_key: auth_key.to_owned(),
        update: true,
    };
    EffectFuture::Concurrent(
        fetch_api::<E, _, _, _>(&request)
            .map_err(CtxError::from)
            .and_then(|result| match result {
                APIResult::Ok { result } => future::ok(result),
                APIResult::Err { error } => future::err(CtxError::from(error)),
            })
            .map_ok(|CollectionResponse { addons, .. }| addons)
            .map(move |result| Msg::Internal(Internal::AddonsAPIResult(request, result)))
            .boxed_env(),
    )
    .into()
}

fn push_profile_to_storage<E: Env + 'static>(profile: &Profile) -> Effect {
    EffectFuture::Sequential(
        E::set_storage(PROFILE_STORAGE_KEY, Some(profile))
            .map(enclose!((profile.uid() => uid) move |result| match result {
                Ok(_) => Msg::Event(Event::ProfilePushedToStorage { uid }),
                Err(error) => Msg::Event(Event::Error {
                    error: CtxError::from(error),
                    source: Box::new(Event::ProfilePushedToStorage { uid }),
                })
            }))
            .boxed_env(),
    )
    .into()
}

fn pull_community_addons<E: Env + 'static>() -> Effect {
    let request = Request::get(CINEMETA_COMMUNITY_ADDONS_URL.as_str())
        .body(())
        .expect("request builder failed");
    EffectFuture::Concurrent(
        E::fetch::<_, CommunityAddonsResp>(request)
            .map_ok(|resp| resp.addons)
            .map_err(CtxError::from)
            .map(|result| Msg::Internal(Internal::AddonsCommunityResult(result)))
            .boxed_env(),
    )
    .into()
}

fn addon_upgrade_error_effects(addon: &Descriptor, error: OtherError) -> Effects {
    addon_action_error_effects(
        error,
        Event::AddonUpgraded {
            transport_url: addon.transport_url.to_owned(),
            id: addon.manifest.id.to_owned(),
        },
    )
}

fn addon_uninstall_error_effects(addon: &Descriptor, error: OtherError) -> Effects {
    addon_action_error_effects(
        error,
        Event::AddonUninstalled {
            transport_url: addon.transport_url.to_owned(),
            id: addon.manifest.id.to_owned(),
        },
    )
}

fn addon_install_error_effects(addon: &Descriptor, error: OtherError) -> Effects {
    addon_action_error_effects(
        error,
        Event::AddonInstalled {
            transport_url: addon.transport_url.to_owned(),
            id: addon.manifest.id.to_owned(),
        },
    )
}

fn addon_action_error_effects(error: OtherError, source: Event) -> Effects {
    Effects::msg(Msg::Event(Event::Error {
        error: CtxError::from(error),
        source: Box::new(source),
    }))
    .unchanged()
}
