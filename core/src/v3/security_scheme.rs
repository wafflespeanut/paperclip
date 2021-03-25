use super::v2;

impl From<v2::SecurityScheme> for openapiv3::SecurityScheme {
    fn from(v2: v2::SecurityScheme) -> Self {
        match v2.type_.as_str() {
            "basic" => openapiv3::SecurityScheme::HTTP {
                scheme: "basic".to_string(),
                bearer_format: None,
            },
            "apiKey" => {
                // how to determine when it should be JWT?
                if v2.in_ == Some("header".into()) {
                    openapiv3::SecurityScheme::HTTP {
                        scheme: "bearer".to_string(),
                        bearer_format: Some("JWT".into()),
                    }
                } else {
                    openapiv3::SecurityScheme::APIKey {
                        location: match v2.in_.unwrap_or_default().as_str() {
                            "query" => openapiv3::APIKeyLocation::Query,
                            "header" => openapiv3::APIKeyLocation::Header,
                            _ => openapiv3::APIKeyLocation::Query,
                        },
                        name: v2.name.unwrap_or_default(),
                    }
                }
            }
            "oauth2" => {
                let scopes = v2
                    .scopes
                    .iter()
                    .fold(indexmap::IndexMap::new(), |mut i, (k, v)| {
                        i.insert(k.clone(), v.clone());
                        i
                    });
                let flow = v2.flow.unwrap_or_default();
                openapiv3::SecurityScheme::OAuth2 {
                    flows: openapiv3::OAuth2Flows {
                        implicit: match flow.as_str() {
                            "implicit" => Some(openapiv3::OAuth2Flow::Implicit {
                                authorization_url: v2.auth_url.clone().unwrap_or_default(),
                                refresh_url: None,
                                scopes: scopes.clone(),
                            }),
                            _ => None,
                        },
                        password: match flow.as_str() {
                            "password" => Some(openapiv3::OAuth2Flow::Password {
                                refresh_url: None,
                                token_url: v2.token_url.clone().unwrap_or_default(),
                                scopes: scopes.clone(),
                            }),
                            _ => None,
                        },
                        client_credentials: match flow.as_str() {
                            "application" => Some(openapiv3::OAuth2Flow::ClientCredentials {
                                refresh_url: None,
                                token_url: v2.token_url.clone().unwrap_or_default(),
                                scopes: scopes.clone(),
                            }),
                            _ => None,
                        },
                        authorization_code: match flow.as_str() {
                            "accessCode" => Some(openapiv3::OAuth2Flow::AuthorizationCode {
                                authorization_url: v2.auth_url.clone().unwrap_or_default(),
                                token_url: v2.token_url.clone().unwrap_or_default(),
                                refresh_url: None,
                                scopes,
                            }),
                            _ => None,
                        },
                    },
                }
            }
            type_ => {
                debug_assert!(false, "Invalid Security Type: {}", type_);
                openapiv3::SecurityScheme::HTTP {
                    scheme: "invalid".to_string(),
                    bearer_format: None,
                }
            }
        }
    }
}
