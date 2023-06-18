use crate::addon_transport::http_transport::legacy::AddonLegacyTransport;
use crate::addon_transport::AddonTransport;
use crate::constants::{ADDON_LEGACY_PATH, ADDON_MANIFEST_PATH, URI_COMPONENT_ENCODE_SET};
use crate::runtime::{Env, EnvError, EnvFutureExt, TryEnvFuture};
use crate::types::addon::{Manifest, ResourcePath, ResourceResponse};
use crate::types::query_params_encode;
use futures::future;
use http::Request;
use percent_encoding::utf8_percent_encode;
use std::marker::PhantomData;
use url::Url;

pub struct AddonHTTPTransport<E: Env> {
    transport_url: Url,
    env: PhantomData<E>,
}

impl<E: Env> AddonHTTPTransport<E> {
    pub fn new(transport_url: Url) -> Self {
        AddonHTTPTransport {
            transport_url,
            env: PhantomData,
        }
    }
}

impl<E: Env> AddonTransport for AddonHTTPTransport<E> {
    /// Request a resource from the addon.
    ///
    /// This will encode all components with [`utf8_percent_encode(.., URI_COMPONENT_ENCODE_SET)`](utf8_percent_encode)
    /// and the [`ResourcePath.extra`](ResourcePath::extra) properties if they are not empty
    /// with [`query_params_encode`].
    fn resource(&self, path: &ResourcePath) -> TryEnvFuture<ResourceResponse> {
        if self.transport_url.path().ends_with(ADDON_LEGACY_PATH) {
            return AddonLegacyTransport::<E>::new(&self.transport_url).resource(path);
        }
        if !self.transport_url.path().ends_with(ADDON_MANIFEST_PATH) {
            return future::err(EnvError::AddonTransport(format!(
                "addon http transport url must ends with {ADDON_MANIFEST_PATH}"
            )))
            .boxed_env();
        }
        let path = if path.extra.is_empty() {
            format!(
                "/{}/{}/{}.json",
                utf8_percent_encode(&path.resource, URI_COMPONENT_ENCODE_SET),
                utf8_percent_encode(&path.r#type, URI_COMPONENT_ENCODE_SET),
                utf8_percent_encode(&path.id, URI_COMPONENT_ENCODE_SET),
            )
        } else {
            format!(
                "/{}/{}/{}/{}.json",
                utf8_percent_encode(&path.resource, URI_COMPONENT_ENCODE_SET),
                utf8_percent_encode(&path.r#type, URI_COMPONENT_ENCODE_SET),
                utf8_percent_encode(&path.id, URI_COMPONENT_ENCODE_SET),
                query_params_encode(path.extra.iter().map(|ev| (&ev.name, &ev.value)))
            )
        };
        let url = self
            .transport_url
            .as_str()
            .replace(ADDON_MANIFEST_PATH, &path);
        let request = Request::get(&url).body(()).expect("request builder failed");
        E::fetch(request)
    }
    fn manifest(&self) -> TryEnvFuture<Manifest> {
        if self.transport_url.path().ends_with(ADDON_LEGACY_PATH) {
            return AddonLegacyTransport::<E>::new(&self.transport_url).manifest();
        }

        let request = Request::get(self.transport_url.as_str())
            .body(())
            .expect("request builder failed");
        E::fetch(request)
    }
}
