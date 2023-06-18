use crate::types::profile::{GDPRConsent, User};
use crate::unit_tests::serde::default_tokens_ext::DefaultTokens;
use chrono::{TimeZone, Utc};
use serde_test::{assert_de_tokens, assert_tokens, Token};

#[test]
fn user() {
    assert_tokens(
        &vec![
            User {
                id: "id".to_owned(),
                email: "email".to_owned(),
                fb_id: Some("fb_id".to_owned()),
                avatar: Some("avatar".to_owned()),
                last_modified: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                date_registered: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                trakt: None,
                premium_expire: None,
                gdpr_consent: GDPRConsent::default(),
            },
            User {
                id: "id".to_owned(),
                email: "email".to_owned(),
                fb_id: None,
                avatar: None,
                last_modified: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                date_registered: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
                trakt: None,
                premium_expire: None,
                gdpr_consent: GDPRConsent::default(),
            },
        ],
        &[
            vec![
                Token::Seq { len: Some(2) },
                Token::Struct {
                    name: "User",
                    len: 9,
                },
                Token::Str("_id"),
                Token::Str("id"),
                Token::Str("email"),
                Token::Str("email"),
                Token::Str("fbId"),
                Token::Some,
                Token::Str("fb_id"),
                Token::Str("avatar"),
                Token::Some,
                Token::Str("avatar"),
                Token::Str("lastModified"),
                Token::Str("2020-01-01T00:00:00Z"),
                Token::Str("dateRegistered"),
                Token::Str("2020-01-01T00:00:00Z"),
                Token::Str("trakt"),
                Token::None,
                Token::Str("premium_expire"),
                Token::None,
                Token::Str("gdpr_consent"),
            ],
            GDPRConsent::default_tokens(),
            vec![
                Token::StructEnd,
                Token::Struct {
                    name: "User",
                    len: 9,
                },
                Token::Str("_id"),
                Token::Str("id"),
                Token::Str("email"),
                Token::Str("email"),
                Token::Str("fbId"),
                Token::None,
                Token::Str("avatar"),
                Token::None,
                Token::Str("lastModified"),
                Token::Str("2020-01-01T00:00:00Z"),
                Token::Str("dateRegistered"),
                Token::Str("2020-01-01T00:00:00Z"),
                Token::Str("trakt"),
                Token::None,
                Token::Str("premium_expire"),
                Token::None,
                Token::Str("gdpr_consent"),
            ],
            GDPRConsent::default_tokens(),
            vec![Token::StructEnd, Token::SeqEnd],
        ]
        .concat(),
    );
    assert_de_tokens(
        &User {
            id: "id".to_owned(),
            email: "email".to_owned(),
            fb_id: None,
            avatar: None,
            last_modified: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
            date_registered: Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(),
            trakt: None,
            premium_expire: None,
            gdpr_consent: GDPRConsent::default(),
        },
        &[
            vec![
                Token::Struct {
                    name: "User",
                    len: 5,
                },
                Token::Str("_id"),
                Token::Str("id"),
                Token::Str("email"),
                Token::Str("email"),
                Token::Str("lastModified"),
                Token::Str("2020-01-01T00:00:00Z"),
                Token::Str("dateRegistered"),
                Token::Str("2020-01-01T00:00:00Z"),
                Token::Str("gdpr_consent"),
            ],
            GDPRConsent::default_tokens(),
            vec![Token::StructEnd],
        ]
        .concat(),
    );
}
