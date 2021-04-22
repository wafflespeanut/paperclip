#[macro_use]
extern crate serde;
#[macro_use]
extern crate serde_json;

use actix_rt::System;
use actix_service::ServiceFactory;
use actix_web::{
    dev::{MessageBody, Payload, ServiceRequest, ServiceResponse},
    App, Error, FromRequest, HttpRequest, HttpServer, Responder,
};
use futures::future::{ok as fut_ok, ready, Future, Ready};
use once_cell::sync::Lazy;
use paperclip::{
    actix::{
        api_v2_errors, api_v2_errors_overlay, api_v2_operation, delete, get, post, put, web,
        Apiv2Schema, Apiv2Security, CreatedJson, NoContent, OpenApiExt,
    },
    v2::models::{DefaultApiRaw, Info, Tag},
};
use parking_lot::Mutex;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::mpsc,
    thread,
};
use uuid_dev::Uuid;

static CLIENT: Lazy<reqwest::blocking::Client> = Lazy::new(|| reqwest::blocking::Client::new());
static PORTS: Lazy<Mutex<HashSet<u16>>> = Lazy::new(|| Mutex::new(HashSet::new()));

type OptionalUuid = Option<uuid_dev::Uuid>;

#[derive(Deserialize, Serialize, Apiv2Schema)]
#[serde(rename_all = "lowercase")]
enum PetClass {
    Dog,
    Cat,
    #[serde(rename = "other")]
    EverythingElse,
}

#[derive(Deserialize, Serialize, Apiv2Schema)]
#[serde(rename_all = "camelCase")]
/// Pets are awesome!
struct Pet {
    /// Pick a good one.
    name: String,
    class: PetClass,
    id: Option<u64>,
    birthday: chrono_dev::NaiveDate,
    updated_on: Option<chrono_dev::NaiveDateTime>,
    #[serde(rename = "uuid")]
    uid: OptionalUuid,
}

impl Default for Pet {
    fn default() -> Self {
        Self {
            name: "".to_string(),
            class: PetClass::EverythingElse,
            birthday: chrono_dev::NaiveDate::from_ymd(2012, 3, 10),
            id: None,
            updated_on: None,
            uid: None,
        }
    }
}

impl Responder for Pet {
    type Error = Error;
    type Future = Ready<Result<actix_web::HttpResponse, Error>>;

    fn respond_to(self, _req: &HttpRequest) -> Self::Future {
        let body = serde_json::to_string(&self).unwrap();

        // Create response and set content type
        ready(Ok(actix_web::HttpResponse::Ok()
            .content_type("application/json")
            .body(body)))
    }
}

#[test]
fn test_simple_app() {
    #[api_v2_operation]
    fn echo_pet(body: web::Json<Pet>) -> impl Future<Output = Result<web::Json<Pet>, Error>> {
        fut_ok(body)
    }

    #[api_v2_operation]
    async fn echo_pet_async(body: web::Json<Pet>) -> Result<web::Json<Pet>, actix_web::Error> {
        Ok(body)
    }

    async fn inner_async_func(body: web::Json<Pet>) -> Pet {
        body.into_inner()
    }

    #[api_v2_operation]
    async fn echo_pet_async_2(body: web::Json<Pet>) -> Result<web::Json<Pet>, actix_web::Error> {
        let pet = inner_async_func(body).await;
        Ok(web::Json(pet))
    }

    #[api_v2_operation]
    fn some_pet(_data: web::Data<String>) -> impl Future<Output = Result<web::Json<Pet>, Error>> {
        #[allow(unreachable_code)]
        fut_ok(unimplemented!())
    }

    #[api_v2_operation]
    async fn adopt_pet() -> Result<CreatedJson<Pet>, ()> {
        let pet: Pet = Pet::default();
        Ok(CreatedJson(pet))
    }

    #[api_v2_operation]
    async fn nothing() -> NoContent {
        NoContent
    }

    fn config(cfg: &mut web::ServiceConfig) {
        cfg.service(web::resource("/echo").route(web::post().to(echo_pet)))
            .service(web::resource("/async_echo").route(web::post().to(echo_pet_async)))
            .service(web::resource("/async_echo_2").route(web::post().to(echo_pet_async_2)))
            .service(web::resource("/adopt").route(web::post().to(adopt_pet)))
            .service(web::resource("/nothing").route(web::get().to(nothing)))
            .service(web::resource("/random").to(some_pet));
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::scope("/api").configure(config))
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                  "info":{"title":"","version":""},
                  "definitions": {
                    "Pet": {
                      "description":"Pets are awesome!",
                      "properties": {
                        "class": {
                          "enum": ["dog", "cat", "other"],
                          "type": "string"
                        },
                        "id": {
                          "format": "int64",
                          "type": "integer"
                        },
                        "name": {
                          "description": "Pick a good one.",
                          "type": "string"
                        },
                        "birthday": {
                          "format": "date",
                          "type": "string"
                        },
                        "updatedOn": {
                          "format": "date-time",
                          "type": "string"
                        },
                        "uuid": {
                          "format": "uuid",
                          "type": "string"
                        }
                      },
                      "required":["birthday", "class", "name"],
                      "type":"object"
                    }
                  },
                  "paths": {
                    "/api/echo": {
                    "post": {
                        "parameters": [{
                            "in": "body",
                            "name": "body",
                            "required": true,
                            "schema": {
                            "$ref": "#/definitions/Pet"
                            }
                        }],
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      }
                    },
                    "/api/async_echo": {
                      "post": {
                        "parameters": [{
                            "in": "body",
                            "name": "body",
                            "required": true,
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }],
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      }
                    },
                    "/api/async_echo_2": {
                      "post": {
                        "parameters": [{
                            "in": "body",
                            "name": "body",
                            "required": true,
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }],
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      }
                    },
                    "/api/random": {
                      "delete": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      },
                      "get": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      },
                      "head": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      },
                      "options": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      },
                      "patch": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      },
                      "post": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      },
                      "put": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      }
                    },
                    "/api/adopt": {
                      "post": {
                        "responses": {
                          "201": {
                            "description": "Created",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }
                        }
                      }
                    },
                    "/api/nothing": {
                      "get": {
                        "responses": {
                          "204": {
                            "description": "No Content"
                          }
                        }
                      }
                    },
                  },
                  "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
#[allow(dead_code)]
fn test_params() {
    #[derive(Deserialize, Apiv2Schema)]
    struct KnownResourceBadge {
        resource: String,
        name: String,
    }

    /// KnownBadge Id Doc
    #[derive(Serialize, Deserialize, Apiv2Schema)]
    struct KnownBadgeId(String);

    /// KnownBadge Id2 Doc
    #[derive(Deserialize, Apiv2Schema)]
    struct KnownBadgeId2(u64);

    /// KnownBadge Id3 Doc
    #[derive(Deserialize, Apiv2Schema)]
    struct KnownBadgeId3(
        /// Number Doc
        pub u64,
        /// String Doc
        pub String,
    );

    #[derive(Deserialize, Apiv2Schema)]
    struct BadgeParams {
        res: Option<u16>,
        colors: Vec<String>,
    }

    #[derive(Deserialize, Apiv2Schema)]
    struct BadgeBody {
        /// JSON value
        json: Option<serde_json::Value>,
        yaml: Option<serde_yaml::Value>,
    }

    #[derive(Deserialize, Apiv2Schema)]
    struct BadgeBodyPatch {
        /// JSON value
        json: Option<serde_json::Value>,
    }

    #[derive(Deserialize, Apiv2Schema)]
    struct BadgeForm {
        data: String,
    }

    #[derive(Deserialize, Apiv2Schema)]
    struct AppState {
        data: String,
    }

    async fn is_data_empty(p: &AppState) -> bool {
        p.data.is_empty()
    }

    // issue: https://github.com/wafflespeanut/paperclip/issues/216
    #[api_v2_operation]
    async fn check_data_ref_async(
        app: web::Data<AppState>,
        _req_data: Option<web::ReqData<bool>>, // this should compile and change nothing
    ) -> web::Json<bool> {
        web::Json(is_data_empty(app.get_ref()).await)
    }

    #[api_v2_operation]
    fn get_resource_2(_p: web::Path<u32>) -> impl Future<Output = &'static str> {
        ready("")
    }

    #[api_v2_operation]
    fn get_known_badge_1(
        _p: web::Path<KnownResourceBadge>,
        _q: web::Query<BadgeParams>,
    ) -> impl Future<Output = &'static str> {
        ready("")
    }

    #[api_v2_operation]
    fn get_known_badge_2(
        _p: web::Path<(u32, String)>,
        _q: web::Query<BadgeParams>,
    ) -> impl Future<Output = &'static str> {
        ready("")
    }

    #[api_v2_operation]
    fn get_known_badge_3(
        _p: web::Path<KnownBadgeId>,
    ) -> impl Future<Output = Result<web::Json<KnownBadgeId>, ()>> {
        futures::future::ok(web::Json(KnownBadgeId("id".into())))
    }

    #[api_v2_operation]
    fn get_known_badge_4(
        _p1: web::Path<KnownBadgeId>,
        _p2: web::Path<(KnownBadgeId2, KnownBadgeId3)>,
    ) -> impl Future<Output = &'static str> {
        ready("")
    }

    #[api_v2_operation]
    fn post_badge_1(
        _p: web::Path<KnownResourceBadge>,
        _q: web::Query<BadgeParams>,
        _f: web::Form<BadgeForm>,
    ) -> impl Future<Output = &'static str> {
        ready("")
    }

    #[api_v2_operation]
    fn post_badge_2(
        _p: web::Path<(u32, String)>,
        _b: web::Json<BadgeBody>,
    ) -> impl Future<Output = &'static str> {
        ready("")
    }

    #[api_v2_operation]
    fn post_badge_3(
        _p: web::Path<u32>,
        _b: web::Json<BadgeBody>,
    ) -> impl Future<Output = &'static str> {
        ready("")
    }

    #[api_v2_operation]
    fn patch_badge_3(
        _p: web::Path<u32>,
        _b: web::Json<BadgeBodyPatch>,
    ) -> impl Future<Output = &'static str> {
        ready("")
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(
                    web::scope("/api")
                        .service(
                            web::resource("/v1/{resource}/v/{name}")
                                .route(web::Route::new().to(get_known_badge_1))
                                .route(web::post().to(post_badge_1)),
                        )
                        .service(
                            // Test that we can also have parameters in scopes
                            web::scope("/v2/{resource}")
                                .service(
                                    web::resource("/v/{name}")
                                        .route(web::get().to(get_known_badge_2))
                                        .route(web::post().to(post_badge_2)),
                                )
                                .service(
                                    web::resource("/v/{id}")
                                        .route(web::get().to(get_known_badge_3)),
                                )
                                .service(
                                    web::resource("/v/{id}/{id2}/{id3}/{id4}")
                                        .route(web::get().to(get_known_badge_4)),
                                )
                                .service(
                                    web::resource("/v")
                                        .route(web::post().to(post_badge_3))
                                        .route(web::patch().to(patch_badge_3)),
                                )
                                .service(
                                    web::resource("/foo").route(web::get().to(get_resource_2)),
                                ),
                        )
                        .service(
                            web::resource("/v2/check_data")
                                .route(web::get().to(check_data_ref_async)),
                        ),
                )
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                    "definitions": {
                        "BadgeBody": {
                            "properties": {
                                "json": {
                                    "description": "JSON value",
                                },
                                "yaml": {
                                }
                            },
                            "type":"object"
                        },
                        "BadgeBodyPatch": {
                            "properties": {
                                "json": {
                                    "description": "JSON value",
                                }
                            },
                            "type":"object"
                        },
                        "KnownBadgeId": {
                            "description": "KnownBadge Id Doc",
                            "type": "string"
                        },
                    },
                    "info": {
                        "title": "",
                        "version": ""
                    },
                    "paths": {
                        "/api/v1/{resource}/v/{name}": {
                            "delete": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "query",
                                        "items": {
                                            "type": "string"
                                        },
                                        "name": "colors",
                                        "required": true,
                                        "type": "array"
                                    },
                                    {
                                        "format": "int32",
                                        "in": "query",
                                        "name": "res",
                                        "type": "integer"
                                    }
                                ],
                                "responses": {
                                }
                            },
                            "get": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "query",
                                        "items": {
                                            "type": "string"
                                        },
                                        "name": "colors",
                                        "required": true,
                                        "type": "array"
                                    },
                                    {
                                        "format": "int32",
                                        "in": "query",
                                        "name": "res",
                                        "type": "integer"
                                    }
                                ],
                                "responses": {
                                }
                            },
                            "head": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "query",
                                        "items": {
                                            "type": "string"
                                        },
                                        "name": "colors",
                                        "required": true,
                                        "type": "array"
                                    },
                                    {
                                        "format": "int32",
                                        "in": "query",
                                        "name": "res",
                                        "type": "integer"
                                    }
                                ],
                                "responses": {
                                }
                            },
                            "options": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "query",
                                        "items": {
                                            "type": "string"
                                        },
                                        "name": "colors",
                                        "required": true,
                                        "type": "array"
                                    },
                                    {
                                        "format": "int32",
                                        "in": "query",
                                        "name": "res",
                                        "type": "integer"
                                    }
                                ],
                                "responses": {
                                }
                            },
                            "patch": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "query",
                                        "items": {
                                            "type": "string"
                                        },
                                        "name": "colors",
                                        "required": true,
                                        "type": "array"
                                    },
                                    {
                                        "format": "int32",
                                        "in": "query",
                                        "name": "res",
                                        "type": "integer"
                                    }
                                ],
                                "responses": {
                                }
                            },
                            "post": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "query",
                                        "items": {
                                            "type": "string"
                                        },
                                        "name": "colors",
                                        "required": true,
                                        "type": "array"
                                    },
                                    {
                                        "format": "int32",
                                        "in": "query",
                                        "name": "res",
                                        "type": "integer"
                                    },
                                    {
                                        "in": "formData",
                                        "name": "data",
                                        "required": true,
                                        "type": "string"
                                    }
                                ],
                                "responses": {
                                }
                            },
                            "put": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "query",
                                        "items": {
                                            "type": "string"
                                        },
                                        "name": "colors",
                                        "required": true,
                                        "type": "array"
                                    },
                                    {
                                        "format": "int32",
                                        "in": "query",
                                        "name": "res",
                                        "type": "integer"
                                    }
                                ],
                                "responses": {
                                }
                            }
                        },
                        "/api/v2/{resource}/foo": {
                            "get": {
                                "parameters": [
                                    {
                                        "format": "int32",
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "integer"
                                    }
                                ],
                                "responses": {
                                }
                            }
                        },
                        "/api/v2/{resource}/v": {
                            "patch": {
                                "parameters": [
                                    {
                                        "format": "int32",
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "integer"
                                    },
                                    {
                                        "in": "body",
                                        "name": "body",
                                        "required": true,
                                        "schema": {
                                            "$ref": "#/definitions/BadgeBodyPatch"
                                        }
                                    }
                                ],
                                "responses": {
                                }
                            },
                            "post": {
                                "parameters": [
                                    {
                                        "format": "int32",
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "integer"
                                    },
                                    {
                                        "in": "body",
                                        "name": "body",
                                        "required": true,
                                        "schema": {
                                            "$ref": "#/definitions/BadgeBody"
                                        }
                                    }
                                ],
                                "responses": {
                                }
                            }
                        },
                        "/api/v2/{resource}/v/{id}": {
                            "get": {
                                "parameters": [
                                    {
                                        "description": "KnownBadge Id Doc",
                                        "in": "path",
                                        "name": "id",
                                        "required": true,
                                        "type": "string"
                                    }
                                ],
                                "responses": {
                                    "200": {
                                        "description": "OK",
                                        "schema": {
                                            "$ref": "#/definitions/KnownBadgeId"
                                        }
                                    }
                                }
                            }
                        },
                        "/api/v2/{resource}/v/{id}/{id2}/{id3}/{id4}": {
                            "get": {
                                "parameters": [
                                    {
                                        "description": "KnownBadge Id Doc",
                                        "in": "path",
                                        "name": "id",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "description": "KnownBadge Id2 Doc",
                                        "format": "int64",
                                        "in": "path",
                                        "name": "id2",
                                        "required": true,
                                        "type": "integer"
                                    },
                                    {
                                        "description": "Number Doc",
                                        "format": "int64",
                                        "in": "path",
                                        "name": "id3",
                                        "required": true,
                                        "type": "integer"
                                    },
                                    {
                                        "description": "String Doc",
                                        "in": "path",
                                        "name": "id4",
                                        "required": true,
                                        "type": "string"
                                    },
                                ],
                                "responses": {
                                }
                            }
                        },
                        "/api/v2/{resource}/v/{name}": {
                            "get": {
                                "parameters": [
                                    {
                                        "format": "int32",
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "integer"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "query",
                                        "items": {
                                            "type": "string"
                                        },
                                        "name": "colors",
                                        "required": true,
                                        "type": "array"
                                    },
                                    {
                                        "format": "int32",
                                        "in": "query",
                                        "name": "res",
                                        "type": "integer"
                                    }
                                ],
                                "responses": {
                                }
                            },
                            "post": {
                                "parameters": [
                                    {
                                        "format": "int32",
                                        "in": "path",
                                        "name": "resource",
                                        "required": true,
                                        "type": "integer"
                                    },
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "body",
                                        "name": "body",
                                        "required": true,
                                        "schema": {
                                            "$ref": "#/definitions/BadgeBody"
                                        }
                                    }
                                ],
                                "responses": {
                                }
                            }
                        },
                        "/api/v2/check_data": {
                            "get": {
                                "responses": {
                                    "200":{
                                        "description": "OK",
                                        "schema":{
                                           "type": "boolean"
                                        }
                                     }
                                }
                            }
                        },
                    },
                    "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
fn test_map_in_out() {
    #[derive(Deserialize, Serialize, Apiv2Schema)]
    struct ImageId(u64);

    #[derive(Serialize, Apiv2Schema)]
    struct Image {
        data: String,
        id: ImageId,
    }

    #[derive(Deserialize, Apiv2Schema)]
    struct Filter {
        pub folders: HashMap<String, Vec<ImageId>>,
    }

    #[derive(Serialize, Apiv2Schema)]
    struct Catalogue {
        pub folders: HashMap<Uuid, Vec<Image>>,
    }

    #[api_v2_operation]
    fn some_images() -> impl Future<Output = web::Json<BTreeMap<String, Image>>> {
        #[allow(unreachable_code)]
        ready(unimplemented!())
    }

    #[api_v2_operation]
    fn catalogue(_filter: web::Json<Filter>) -> impl Future<Output = web::Json<Catalogue>> {
        #[allow(unreachable_code)]
        ready(unimplemented!())
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::resource("/images").route(web::get().to(some_images)))
                .service(web::resource("/catalogue").route(web::post().to(catalogue)))
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                    "definitions":{
                        "Catalogue":{
                           "properties":{
                              "folders":{
                                 "additionalProperties":{
                                    "items":{
                                       "properties":{
                                          "data":{
                                             "type":"string"
                                          },
                                          "id":{
                                             "format":"int64",
                                             "type":"integer"
                                          }
                                       },
                                       "required":[
                                          "data",
                                          "id"
                                       ],
                                       "type":"object"
                                    },
                                    "type":"array"
                                 },
                                 "type":"object"
                              }
                           },
                           "required":[
                              "folders"
                           ],
                           "type":"object"
                        },
                        "Filter":{
                           "properties":{
                              "folders":{
                                 "additionalProperties":{
                                    "items":{
                                       "format":"int64",
                                       "type":"integer"
                                    },
                                    "type":"array"
                                 },
                                 "type":"object"
                              }
                           },
                           "required":[
                              "folders"
                           ],
                           "type":"object"
                        },
                        "Image":{
                           "properties":{
                              "data":{
                                 "type":"string"
                              },
                              "id":{
                                 "format":"int64",
                                 "type":"integer"
                              }
                           },
                           "required":[
                              "data",
                              "id"
                           ],
                           "type":"object"
                        }
                     },
                     "info":{
                        "title":"",
                        "version":""
                     },
                     "paths":{
                        "/catalogue":{
                           "post":{
                              "parameters":[
                                 {
                                    "in":"body",
                                    "name":"body",
                                    "required":true,
                                    "schema":{
                                       "$ref":"#/definitions/Filter"
                                    }
                                 }
                              ],
                              "responses":{
                                 "200":{
                                    "description":"OK",
                                    "schema":{
                                       "$ref":"#/definitions/Catalogue"
                                    }
                                 }
                              }
                           }
                        },
                        "/images":{
                           "get":{
                              "responses":{
                                 "200":{
                                    "description":"OK",
                                    "schema":{
                                       "additionalProperties":{
                                          "$ref":"#/definitions/Image"
                                       },
                                       "type":"object"
                                    }
                                 }
                              }
                           }
                        }
                     },
                     "swagger":"2.0"
                }),
            );
        },
    );
}

#[test]
fn test_serde_flatten() {
    #[derive(Deserialize, Serialize, Apiv2Schema)]
    struct PagedQuery {
        /// First image number to return
        offset: Option<i32>,
        /// Return number of images
        size: Option<i32>,
    }

    #[derive(Deserialize, Serialize, Apiv2Schema)]
    struct Paging {
        /// Starting image number
        offset: i32,
        /// Total images found
        total: i32,
        /// Page size
        size: i32,
    }

    #[derive(Serialize, Apiv2Schema)]
    struct Image {
        data: String,
        id: Uuid,
        time: chrono_dev::DateTime<chrono_dev::Utc>,
    }

    /// Images response with paging information embedded
    #[derive(Serialize, Apiv2Schema)]
    struct Images {
        data: Vec<Image>,
        #[serde(flatten)]
        paging: Paging,
    }

    /// Query images from library by name
    #[derive(Deserialize, Apiv2Schema)]
    struct ImagesQuery {
        #[serde(flatten)]
        paging: PagedQuery,
        name: Option<String>,
    }

    #[api_v2_operation]
    async fn some_images(_filter: web::Query<ImagesQuery>) -> Result<web::Json<Images>, ()> {
        #[allow(unreachable_code)]
        if _filter.paging.offset.is_some() && _filter.name.is_some() {
            unimplemented!()
        }
        unimplemented!()
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::resource("/images").route(web::get().to(some_images)))
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                    "definitions": {
                        "Images": {
                          "description": "Images response with paging information embedded",
                          "properties": {
                            "data": {
                              "items": {
                                "properties": {
                                  "data": {
                                    "type": "string"
                                  },
                                  "id": {
                                    "format": "uuid",
                                    "type": "string"
                                  },
                                  "time": {
                                    "format": "date-time",
                                    "type": "string"
                                  }
                                },
                                "required": [
                                  "data",
                                  "id",
                                  "time"
                                ],
                                 "type":"object"
                              },
                              "type": "array"
                            },
                            "offset": {
                              "description": "Starting image number",
                              "format": "int32",
                              "type": "integer"
                            },
                            "size": {
                              "description": "Page size",
                              "format": "int32",
                              "type": "integer"
                            },
                            "total": {
                              "description": "Total images found",
                              "format": "int32",
                              "type": "integer"
                            }
                          },
                          "required": [
                            "data",
                            "paging"
                          ],
                          "type":"object"
                        }
                      },
                      "info": {
                        "title": "",
                        "version": ""
                      },
                      "paths": {
                        "/images": {
                          "get": {
                            "parameters": [
                              {
                                "in": "query",
                                "name": "name",
                                "type": "string"
                              },
                              {
                                "description": "First image number to return",
                                "format": "int32",
                                "in": "query",
                                "name": "offset",
                                "type": "integer"
                              },
                              {
                                "description": "Return number of images",
                                "format": "int32",
                                "in": "query",
                                "name": "size",
                                "type": "integer"
                              }
                            ],
                            "responses": {
                              "200": {
                                "description": "OK",
                                "schema": {
                                  "$ref": "#/definitions/Images"
                                }
                              }
                            }
                          }
                        }
                      },
                      "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
fn test_list_in_out() {
    #[derive(Serialize, Deserialize, Apiv2Schema)]
    enum Sort {
        Asc,
        Desc,
    }

    #[derive(Serialize, Deserialize, Apiv2Schema)]
    struct Params {
        sort: Option<Sort>,
        limit: Option<u16>,
    }

    #[api_v2_operation]
    fn get_pets(_q: web::Query<Params>) -> impl Future<Output = web::Json<Vec<Pet>>> {
        #[allow(unreachable_code)]
        ready(unimplemented!())
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::resource("/pets").route(web::get().to(get_pets)))
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                  "info":{"title":"","version":""},
                  "definitions": {
                    "Pet": {
                      "description": "Pets are awesome!",
                      "properties": {
                        "class": {
                          "enum": ["dog", "cat", "other"],
                          "type": "string"
                        },
                        "id": {
                          "format": "int64",
                          "type": "integer"
                        },
                        "name": {
                          "description": "Pick a good one.",
                          "type": "string"
                        },
                        "birthday": {
                          "format": "date",
                          "type": "string"
                        },
                        "updatedOn": {
                          "format": "date-time",
                          "type": "string"
                        },
                        "uuid": {
                          "format": "uuid",
                          "type": "string"
                        }
                      },
                      "required":["birthday", "class", "name"],
                      "type":"object"
                    }
                  },
                  "paths": {
                    "/pets": {
                      "get": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "type": "array",
                              "items": {
                                "$ref": "#/definitions/Pet"
                              }
                            }
                          }
                        },
                        "parameters": [{
                            "format": "int32",
                            "in": "query",
                            "name": "limit",
                            "type": "integer"
                          }, {
                            "enum": ["Asc", "Desc"],
                            "in": "query",
                            "name": "sort",
                            "type": "string"
                        }],
                      },
                    }
                  },
                  "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
fn test_tags() {
    #[derive(Serialize, Apiv2Schema)]
    struct Image {
        data: String,
        id: u64,
    }

    #[api_v2_operation(tags(Cats, Dogs))]
    fn some_pets_images() -> impl Future<Output = web::Json<Vec<Image>>> {
        ready(web::Json(Vec::new()))
    }

    #[api_v2_operation(tags(Cats, "Nice cars"))]
    fn some_cats_cars_images() -> impl Future<Output = web::Json<Vec<Image>>> {
        ready(web::Json(Vec::new()))
    }

    run_and_check_app(
        || {
            let mut spec = DefaultApiRaw::default();
            spec.tags = vec![
                Tag {
                    name: "Dogs".to_string(),
                    description: Some("Images of dogs".to_string()),
                    external_docs: None,
                },
                Tag {
                    name: "Cats".to_string(),
                    description: Some("Images of cats".to_string()),
                    external_docs: None,
                },
                Tag {
                    name: "Nice cars".to_string(),
                    description: Some("Images of nice cars".to_string()),
                    external_docs: None,
                },
            ];
            spec.info = Info {
                version: "0.1".into(),
                title: "Image server".into(),
                ..Default::default()
            };

            App::new()
                .wrap_api_with_spec(spec)
                .with_json_spec_at("/api/spec")
                .service(web::resource("/images/pets").route(web::get().to(some_pets_images)))
                .service(
                    web::resource("/images/cats/cars").route(web::get().to(some_cats_cars_images)),
                )
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                    "definitions":{
                        "Image":{
                            "properties":{
                                "data":{
                                "type":"string"
                                },
                                "id":{
                                "format":"int64",
                                "type":"integer"
                                }
                            },
                            "required":[
                                "data",
                                "id"
                            ],
                            "type":"object"
                        }
                    },
                    "info":{
                        "title":"Image server",
                        "version":"0.1"
                    },
                    "paths":{
                        "/images/pets":{
                            "get":{
                                "responses":{
                                "200":{
                                    "description":"OK",
                                    "schema":{
                                        "items":{
                                            "$ref":"#/definitions/Image"
                                        },
                                        "type":"array"
                                    }
                                }
                                },
                                "tags":[ "Cats", "Dogs" ]
                            }
                        },
                        "/images/cats/cars":{
                            "get":{
                                "responses":{
                                "200":{
                                    "description":"OK",
                                    "schema":{
                                        "items":{
                                            "$ref":"#/definitions/Image"
                                        },
                                        "type":"array"
                                    }
                                }
                                },
                                "tags":[ "Cats", "Nice cars" ]
                            }
                        }
                    },
                    "swagger":"2.0",
                    "tags":[
                        {
                            "description":"Images of dogs",
                            "name":"Dogs"
                        },
                        {
                            "description":"Images of cats",
                            "name":"Cats"
                        },
                        {
                            "description":"Images of nice cars",
                            "name":"Nice cars"
                        }
                    ]
                }),
            );
        },
    );
}

#[test]
#[allow(unreachable_code)]
fn test_impl_traits() {
    #[api_v2_operation]
    fn index() -> impl Responder {
        ""
    }

    #[derive(Serialize, Deserialize, Apiv2Schema)]
    struct Params {
        limit: Option<u16>,
    }

    #[api_v2_operation]
    fn get_pets(
        _data: web::Data<String>,
        _q: web::Query<Params>,
    ) -> impl Future<Output = Result<web::Json<Vec<Pet>>, ()>> {
        if true {
            // test for return in wrapper blocks (#75)
            return futures::future::err(());
        }

        futures::future::err(())
    }

    #[api_v2_operation]
    async fn get_pet_async() -> impl Responder {
        Pet::default()
    }

    #[api_v2_operation]
    fn get_pet() -> impl Responder {
        Pet::default()
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::resource("/").route(web::get().to(index)))
                .service(web::resource("/pets").route(web::get().to(get_pets)))
                .service(web::resource("/pet").route(web::get().to(get_pet)))
                .service(web::resource("/pet_async").route(web::get().to(get_pet_async)))
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                  "info":{"title":"","version":""},
                  "definitions": {
                    "Pet": {
                      "description": "Pets are awesome!",
                      "properties": {
                        "class": {
                          "enum": ["dog", "cat", "other"],
                          "type": "string"
                        },
                        "id": {
                          "format": "int64",
                          "type": "integer"
                        },
                        "name": {
                          "description": "Pick a good one.",
                          "type": "string"
                        },
                        "birthday": {
                          "format": "date",
                          "type": "string"
                        },
                        "updatedOn": {
                          "format": "date-time",
                          "type": "string"
                        },
                        "uuid": {
                          "format": "uuid",
                          "type": "string"
                        }
                      },
                      "required":["birthday", "class", "name"],
                      "type":"object"
                    }
                  },
                  "paths": {
                    "/": {
                      "get": {
                        "responses": {}
                      }
                    },
                    "/pets": {
                      "get": {
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "type": "array",
                              "items": {
                                "$ref": "#/definitions/Pet"
                              }
                            }
                          }
                        },
                        "parameters": [{
                            "format": "int32",
                            "in": "query",
                            "name": "limit",
                            "type": "integer"
                        }]
                      },
                    },
                    "/pet": {
                      "get": {
                        "responses": {}
                      }
                    },
                    "/pet_async": {
                      "get": {
                        "responses": {}
                      }
                    }
                  },
                  "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
#[allow(unreachable_code)]
fn test_operation_with_generics() {
    #[api_v2_operation]
    fn get_pet_by_id<I: paperclip::v2::schema::Apiv2Schema>(
        _path: web::Path<I>,
    ) -> impl Future<Output = Result<web::Json<Vec<Pet>>, ()>> {
        futures::future::ok(web::Json(vec![Pet::default()]))
    }

    #[api_v2_operation]
    async fn get_pet_by_name<S: paperclip::v2::schema::Apiv2Schema + ToString>(
        _path: web::Path<S>,
    ) -> Result<web::Json<Vec<Pet>>, ()> {
        Ok(web::Json(vec![Pet::default()]))
    }

    #[api_v2_operation]
    async fn get_pet_by_type<S>(_path: web::Path<S>) -> Result<web::Json<Vec<Pet>>, ()>
    where
        S: paperclip::v2::schema::Apiv2Schema + ToString,
    {
        Ok(web::Json(vec![Pet::default()]))
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::resource("/pet/id/{id}").route(web::get().to(get_pet_by_id::<u64>)))
                .service(
                    web::resource("/pet/name/{name}")
                        .route(web::get().to(get_pet_by_name::<String>)),
                )
                .service(
                    web::resource("/pet/type/{type}")
                        .route(web::get().to(get_pet_by_type::<String>)),
                )
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                    "definitions":{
                        "Pet":{
                           "description":"Pets are awesome!",
                           "properties":{
                              "class":{
                                 "enum":[
                                    "dog",
                                    "cat",
                                    "other"
                                 ],
                                 "type":"string"
                              },
                              "id":{
                                 "format":"int64",
                                 "type":"integer"
                              },
                              "name":{
                                 "description":"Pick a good one.",
                                 "type":"string"
                              },
                              "birthday": {
                                "format": "date",
                                "type": "string"
                              },
                              "updatedOn":{
                                 "format":"date-time",
                                 "type":"string"
                              },
                              "uuid":{
                                 "format":"uuid",
                                 "type":"string"
                              }
                           },
                           "required":[
                             "birthday",
                              "class",
                              "name"
                           ],
                           "type":"object"
                        }
                     },
                     "info":{
                        "title":"",
                        "version":""
                     },
                     "paths":{
                        "/pet/id/{id}":{
                           "get":{
                              "responses":{
                                 "200":{
                                    "description":"OK",
                                    "schema":{
                                       "items":{
                                          "$ref":"#/definitions/Pet"
                                       },
                                       "type":"array"
                                    }
                                 }
                              },
                              "parameters":[
                                {
                                   "format":"int64",
                                   "in":"path",
                                   "name":"id",
                                   "required":true,
                                   "type":"integer"
                                }
                             ]
                          },
                        },
                        "/pet/name/{name}":{
                           "get":{
                              "responses":{
                                 "200":{
                                    "description":"OK",
                                    "schema":{
                                       "items":{
                                          "$ref":"#/definitions/Pet"
                                       },
                                       "type":"array"
                                    }
                                 }
                              },
                              "parameters":[
                                {
                                   "in":"path",
                                   "name":"name",
                                   "required":true,
                                   "type":"string"
                                }
                             ]
                          },
                        },
                        "/pet/type/{type}":{
                           "get":{
                              "responses":{
                                 "200":{
                                    "description":"OK",
                                    "schema":{
                                       "items":{
                                          "$ref":"#/definitions/Pet"
                                       },
                                       "type":"array"
                                    }
                                 }
                              },
                              "parameters":[
                                {
                                   "in":"path",
                                   "name":"type",
                                   "required":true,
                                   "type":"string"
                                }
                             ]
                          },
                        }
                     },
                     "swagger":"2.0"
                }),
            );
        },
    );
}

#[test]
#[allow(unreachable_code)]
fn test_operations_documentation() {
    /// Index call
    #[api_v2_operation]
    fn index() -> impl Responder {
        ""
    }

    #[derive(Serialize, Deserialize, Apiv2Schema)]
    struct Params {
        limit: Option<u16>,
    }

    /// List all pets
    ///
    /// Will provide list of all pets available for sale
    #[api_v2_operation]
    fn get_pets(
        _data: web::Data<String>,
        _q: web::Query<Params>,
    ) -> impl Future<Output = Result<web::Json<Vec<Pet>>, ()>> {
        if true {
            // test for return in wrapper blocks (#75)
            return futures::future::err(());
        }

        futures::future::err(())
    }

    /// Get pet info
    ///
    /// Will provide details on a pet
    #[api_v2_operation]
    async fn get_pet_async() -> impl Responder {
        Pet::default()
    }

    /// Get pet info
    /// sync version
    #[api_v2_operation]
    fn get_pet() -> impl Responder {
        Pet::default()
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::resource("/").route(web::get().to(index)))
                .service(web::resource("/pets").route(web::get().to(get_pets)))
                .service(web::resource("/pet").route(web::get().to(get_pet)))
                .service(web::resource("/pet_async").route(web::get().to(get_pet_async)))
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                  "info":{"title":"","version":""},
                  "definitions": {
                    "Pet": {
                      "description": "Pets are awesome!",
                      "properties": {
                        "class": {
                          "enum": ["dog", "cat", "other"],
                          "type": "string"
                        },
                        "id": {
                          "format": "int64",
                          "type": "integer"
                        },
                        "name": {
                          "description": "Pick a good one.",
                          "type": "string"
                        },
                        "birthday": {
                          "format": "date",
                          "type": "string"
                        },
                        "updatedOn": {
                          "format": "date-time",
                          "type": "string"
                        },
                        "uuid": {
                          "format": "uuid",
                          "type": "string"
                        }
                      },
                      "required":["birthday", "class", "name"],
                      "type":"object"
                    }
                  },
                  "paths": {
                    "/": {
                      "get": {
                        "responses": {},
                        "summary":"Index call"
                      }
                    },
                    "/pets": {
                      "get": {
                        "description":"Will provide list of all pets available for sale",
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "type": "array",
                              "items": {
                                "$ref": "#/definitions/Pet"
                              }
                            }
                          }
                        },
                        "summary":"List all pets",
                        "parameters": [{
                            "format": "int32",
                            "in": "query",
                            "name": "limit",
                            "type": "integer"
                        }]
                      },
                    },
                    "/pet": {
                      "get": {
                        "responses": {},
                        "summary":"Get pet info sync version"
                      }
                    },
                    "/pet_async": {
                      "get": {
                        "responses": {},
                        "description":"Will provide details on a pet",
                        "summary":"Get pet info"
                      }
                    }
                  },
                  "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
#[allow(unreachable_code)]
fn test_operations_macro_attributes() {
    /// Index operation
    ///
    /// This doc comment will be overriden by macro attrs
    #[api_v2_operation(
        summary = "Root",
        description = "Provides an empty value in response",
        operation_id = "getIndex",
        consumes = "application/json, text/plain",
        produces = "text/plain"
    )]
    fn index() -> impl Responder {
        ""
    }

    #[derive(Serialize, Deserialize, Apiv2Schema)]
    struct Params {
        limit: Option<u16>,
    }

    /// List all pets (in summary)
    ///
    /// This doc comment will be used in description
    #[api_v2_operation(operation_id = "getPets")]
    fn get_pets(
        _data: web::Data<String>,
        _q: web::Query<Params>,
    ) -> impl Future<Output = Result<web::Json<Vec<Pet>>, ()>> {
        if true {
            // test for return in wrapper blocks (#75)
            return futures::future::err(());
        }

        futures::future::err(())
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::resource("/").route(web::get().to(index)))
                .service(web::resource("/pets").route(web::get().to(get_pets)))
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                    "definitions": {
                        "Pet": {
                            "description": "Pets are awesome!",
                            "properties": {
                                "class": {
                                "enum": ["dog", "cat", "other"],
                                    "type":"string"
                                },
                                "id": {
                                    "format": "int64",
                                    "type": "integer"
                                },
                                "name": {
                                    "description": "Pick a good one.",
                                    "type": "string"
                                },
                                "birthday": {
                                  "format": "date",
                                  "type": "string"
                                },
                                "updatedOn": {
                                    "format": "date-time",
                                    "type": "string"
                                },
                                "uuid":{
                                    "format": "uuid",
                                    "type": "string"
                                }
                            },
                            "required":[
                                "birthday",
                                "class",
                                "name"
                            ],
                            "type":"object"
                        }
                    },
                    "info": {
                        "title":"",
                        "version":""
                    },
                    "paths": {
                        "/": {
                            "get": {
                                "consumes": [
                                    "application/json",
                                    "text/plain"
                                ],
                                "description": "Provides an empty value in response",
                                "operationId": "getIndex",
                                "produces": [ "text/plain" ],
                                "responses": {},
                                "summary": "Root"
                            }
                        },
                        "/pets": {
                            "get": {
                                "description": "This doc comment will be used in description",
                                "operationId": "getPets",
                                "parameters":[{
                                    "format":"int32",
                                    "in":"query",
                                    "name":"limit",
                                    "type":"integer"
                                }],
                                "responses": {
                                "200": {
                                    "description": "OK",
                                    "schema": {
                                        "items": {
                                            "$ref": "#/definitions/Pet"
                                        },
                                        "type": "array"
                                    }
                                }
                                },
                                "summary": "List all pets (in summary)"
                            }
                        }
                    },
                    "swagger": "2.0"
                }),
            );
        },
    );
}

#[test] // issue #71
fn test_multiple_method_routes() {
    #[api_v2_operation]
    fn test_get() -> impl Future<Output = String> {
        ready("get".into())
    }

    #[api_v2_operation]
    fn test_post() -> impl Future<Output = String> {
        ready("post".into())
    }

    fn test_app<F, T, B>(f: F)
    where
        F: Fn() -> App<T, B> + Clone + Send + Sync + 'static,
        B: MessageBody + 'static,
        T: ServiceFactory<
                Config = (),
                Request = ServiceRequest,
                Response = ServiceResponse<B>,
                Error = Error,
                InitError = (),
            > + 'static,
    {
        run_and_check_app(f, |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/v1/foo", addr))
                .send()
                .expect("request failed?");
            assert_eq!(resp.status().as_u16(), 200);
            assert_eq!(resp.text().unwrap(), "get");

            let resp = CLIENT
                .post(&format!("http://{}/v1/foo", addr))
                .send()
                .expect("request failed?");
            assert_eq!(resp.status().as_u16(), 200);
            assert_eq!(resp.text().unwrap(), "post");

            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                  "info":{"title":"","version":""},
                  "definitions": {},
                  "paths": {
                    "/v1/foo": {
                      "get": {
                        "responses": {},
                      },
                      "post": {
                        "responses": {},
                      },
                    }
                  },
                  "swagger": "2.0",
                }),
            );
        });
    }

    test_app(|| {
        App::new()
            .wrap_api()
            .with_json_spec_at("/api/spec")
            .route("/v1/foo", web::get().to(test_get))
            .route("/v1/foo", web::post().to(test_post))
            .build()
    });

    fn config(cfg: &mut web::ServiceConfig) {
        cfg.route("/foo", web::get().to(test_get))
            .route("/foo", web::post().to(test_post));
    }

    test_app(|| {
        App::new()
            .wrap_api()
            .with_json_spec_at("/api/spec")
            .service(web::scope("/v1").configure(config))
            .build()
    });

    fn config_1(cfg: &mut web::ServiceConfig) {
        cfg.route("/v1/foo", web::get().to(test_get));
    }

    fn config_2(cfg: &mut web::ServiceConfig) {
        cfg.route("/v1/foo", web::post().to(test_post));
    }

    test_app(|| {
        App::new()
            .wrap_api()
            .with_json_spec_at("/api/spec")
            .configure(config_1)
            .configure(config_2)
            .build()
    });
}

#[test]
fn test_custom_extractor_empty_schema() {
    #[derive(Apiv2Schema)]
    #[openapi(empty)]
    struct SomeUselessThing<T>(T);

    impl FromRequest for SomeUselessThing<String> {
        type Error = Error;
        type Future = Ready<Result<Self, Self::Error>>;
        type Config = ();

        fn from_request(_req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
            fut_ok(SomeUselessThing(String::from("booya")))
        }
    }

    #[api_v2_operation]
    fn index(
        _req: HttpRequest,
        _payload: String,
        _thing: SomeUselessThing<String>,
    ) -> impl Future<Output = &'static str> {
        ready("")
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .service(web::resource("/").route(web::get().to(index)))
                .with_raw_json_spec(|app, spec| {
                    app.route(
                        "/api/spec",
                        web::get().to(move || actix_web::HttpResponse::Ok().json(&spec)),
                    )
                })
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                  "info":{"title":"","version":""},
                  "definitions": {},
                  "paths": {
                    "/": {
                      "get": {
                        "responses": {}
                      }
                    }
                  },
                  "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
fn test_errors_app() {
    use actix_web::{
        error::{ErrorBadRequest, ResponseError},
        HttpResponse,
    };
    use std::fmt;

    #[derive(Debug, Serialize, Deserialize, Apiv2Schema)]
    struct PetErrorScheme1 {}
    #[derive(Debug, Serialize, Deserialize, Apiv2Schema)]
    struct PetErrorScheme2 {}
    #[derive(Debug, Serialize, Deserialize, Apiv2Schema)]
    struct PetErrorScheme3 {}

    #[api_v2_errors(
        400,
        description = "Sorry, bad request",
        code = 401,
        code = 403,
        schema = "PetErrorScheme1",
        description = "Forbidden, go away",
        500,
        description = "Internal Server Error",
        schema = "PetErrorScheme2"
    )]
    #[derive(Debug)]
    struct PetError {}

    #[api_v2_errors(
        400,
        description = "Sorry, bad request",
        code = 401,
        code = 403,
        schema = "PetErrorScheme1",
        description = "Forbidden, go away",
        500,
        description = "Internal Server Error",
        default_schema = "PetErrorScheme2"
    )]
    #[derive(Debug)]
    struct PetError2 {}

    #[api_v2_errors_overlay(401)]
    #[derive(Debug)]
    struct PetErrorOverlay(pub PetError2);

    impl fmt::Display for PetError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Bad Request")
        }
    }
    impl fmt::Display for PetError2 {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "Bad Request")
        }
    }

    impl ResponseError for PetError {
        fn error_response(&self) -> HttpResponse {
            HttpResponse::from_error(ErrorBadRequest("Bad Request"))
        }
    }
    impl ResponseError for PetError2 {
        fn error_response(&self) -> HttpResponse {
            HttpResponse::from_error(ErrorBadRequest("Bad Request"))
        }
    }

    #[api_v2_operation]
    async fn echo_pet_with_errors(body: web::Json<Pet>) -> Result<web::Json<Pet>, PetError> {
        Ok(body)
    }

    #[api_v2_operation]
    async fn echo_pet_with_errors2(
        body: web::Json<Pet>,
    ) -> Result<web::Json<Pet>, PetErrorOverlay> {
        Ok(body)
    }

    fn config(cfg: &mut web::ServiceConfig) {
        cfg.service(web::resource("/echo").route(web::post().to(echo_pet_with_errors)));
        cfg.service(web::resource("/echo2").route(web::post().to(echo_pet_with_errors2)));
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(web::scope("/api").configure(config))
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                  "info":{"title":"","version":""},
                  "definitions": {
                    "Pet": {
                      "description": "Pets are awesome!",
                      "properties": {
                        "class": {
                          "enum": ["dog", "cat", "other"],
                          "type": "string"
                        },
                        "id": {
                          "format": "int64",
                          "type": "integer"
                        },
                        "name": {
                          "description": "Pick a good one.",
                          "type": "string"
                        },
                        "birthday": {
                          "format": "date",
                          "type": "string"
                        },
                        "updatedOn": {
                          "format": "date-time",
                          "type": "string"
                        },
                        "uuid": {
                          "format": "uuid",
                          "type": "string"
                        }
                      },
                      "required":["birthday", "class", "name"],
                      "type":"object"
                    },
                    "PetErrorScheme1": {
                      "type": "object"
                    },
                    "PetErrorScheme2": {
                      "type": "object"
                    },
                  },
                  "paths": {
                    "/api/echo": {
                      "post": {
                        "parameters": [{
                            "in": "body",
                            "name": "body",
                            "required": true,
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }],
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          },
                          "400": {
                            "description": "Sorry, bad request"
                          },
                          "401": {
                            "description": "Unauthorized"
                          },
                          "403":{
                            "description": "Forbidden, go away",
                            "schema": {
                              "$ref": "#/definitions/PetErrorScheme1"
                            }
                          },
                          "500": {
                            "description": "Internal Server Error",
                            "schema": {
                              "$ref": "#/definitions/PetErrorScheme2"
                            }
                          }
                        }
                      }
                    },
                    "/api/echo2": {
                      "post": {
                        "parameters": [{
                            "in": "body",
                            "name": "body",
                            "required": true,
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }],
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          },
                          "400": {
                            "description": "Sorry, bad request",
                            "schema": {
                              "$ref": "#/definitions/PetErrorScheme2"
                            }
                          },
                          "403":{
                            "description": "Forbidden, go away",
                            "schema": {
                              "$ref": "#/definitions/PetErrorScheme1"
                            }
                          },
                          "500": {
                            "description": "Internal Server Error",
                            "schema": {
                              "$ref": "#/definitions/PetErrorScheme2"
                            }
                          }
                        }
                      }
                    },
                  },
                  "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
fn test_security_app() {
    #[derive(Apiv2Security, Deserialize)]
    #[openapi(
        apiKey,
        alias = "JWT",
        in = "header",
        name = "Authorization",
        description = "Use format 'Bearer TOKEN'"
    )]
    struct AccessToken;

    impl FromRequest for AccessToken {
        type Error = Error;
        type Future = Ready<Result<Self, Self::Error>>;
        type Config = ();

        fn from_request(_: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
            ready(Ok(Self {}))
        }
    }

    #[derive(Apiv2Security, Deserialize)]
    #[openapi(
        oauth2,
        alias = "MyOAuth2",
        auth_url = "http://example.com/",
        token_url = "http://example.com/token",
        flow = "password"
    )]
    struct OAuth2Access;

    impl FromRequest for OAuth2Access {
        type Error = Error;
        type Future = Ready<Result<Self, Self::Error>>;
        type Config = ();

        fn from_request(_: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
            ready(Ok(Self {}))
        }
    }

    #[derive(Apiv2Security, Deserialize)]
    #[openapi(parent = "OAuth2Access", scopes("pets.read", "pets.write"))]
    struct PetScope;

    impl FromRequest for PetScope {
        type Error = Error;
        type Future = Ready<Result<Self, Self::Error>>;
        type Config = ();

        fn from_request(_: &HttpRequest, _payload: &mut actix_web::dev::Payload) -> Self::Future {
            ready(Ok(Self {}))
        }
    }

    #[api_v2_operation]
    async fn echo_pet_with_jwt(_: AccessToken, body: web::Json<Pet>) -> web::Json<Pet> {
        body
    }

    #[api_v2_operation]
    async fn echo_pet_with_petstore(_: PetScope, body: web::Json<Pet>) -> web::Json<Pet> {
        body
    }

    fn config(cfg: &mut web::ServiceConfig) {
        cfg.service(web::resource("/echo1").route(web::post().to(echo_pet_with_jwt)))
            .service(web::resource("/echo2").route(web::post().to(echo_pet_with_petstore)));
    }

    run_and_check_app(
        move || {
            App::new()
                .wrap_api()
                .service(web::scope("/api").configure(config))
                .with_json_spec_at("/spec")
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                  "info":{"title":"","version":""},
                  "definitions": {
                    "Pet": {
                      "description": "Pets are awesome!",
                      "properties": {
                        "class": {
                          "enum": ["dog", "cat", "other"],
                          "type": "string"
                        },
                        "id": {
                          "format": "int64",
                          "type": "integer"
                        },
                        "name": {
                          "description": "Pick a good one.",
                          "type": "string"
                        },
                        "birthday": {
                          "format": "date",
                          "type": "string"
                        },
                        "updatedOn": {
                          "format": "date-time",
                          "type": "string"
                        },
                        "uuid": {
                          "format": "uuid",
                          "type": "string"
                        }
                      },
                      "required":["birthday", "class", "name"],
                      "type":"object"
                    }
                  },
                  "paths": {
                    "/api/echo1": {
                      "post": {
                        "parameters": [{
                            "in": "body",
                            "name": "body",
                            "required": true,
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }],
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          },
                        },
                        "security": [
                          {
                            "JWT": []
                          }
                        ]
                      }
                    },
                    "/api/echo2": {
                      "post": {
                        "parameters": [{
                            "in": "body",
                            "name": "body",
                            "required": true,
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          }],
                        "responses": {
                          "200": {
                            "description": "OK",
                            "schema": {
                              "$ref": "#/definitions/Pet"
                            }
                          },
                        },
                        "security": [
                          {
                            "MyOAuth2": ["pets.read", "pets.write"]
                          }
                        ]
                      }
                    },
                  },
                  "securityDefinitions": {
                    "JWT": {
                        "description":"Use format 'Bearer TOKEN'",
                        "in": "header",
                        "name": "Authorization",
                        "type": "apiKey"
                    },
                    "MyOAuth2": {
                        "scopes": {
                          "pets.read": "pets.read",
                          "pets.write": "pets.write"
                        },
                        "type": "oauth2",
                        "authorizationUrl": "http://example.com/",
                        "tokenUrl": "http://example.com/token",
                        "flow": "password"
                    }
                  },
                  "swagger": "2.0"
                }),
            );
        },
    );
}

#[test]
fn test_method_macro() {
    #[get("/v0/pets")]
    #[api_v2_operation]
    fn get_pets() -> impl Future<Output = Result<web::Json<Vec<Pet>>, ()>> {
        futures::future::ready(Ok(web::Json(Default::default())))
    }
    #[put("/v0/pets/{name}")]
    #[api_v2_operation]
    fn put_pet(
        _name: web::Path<String>,
        pet: web::Json<Pet>,
    ) -> impl Future<Output = Result<web::Json<Pet>, ()>> {
        futures::future::ready(Ok(pet))
    }
    #[post("/v0/pets")]
    #[api_v2_operation]
    fn post_pet(pet: web::Json<Pet>) -> impl Future<Output = Result<web::Json<Pet>, ()>> {
        futures::future::ready(Ok(pet))
    }
    #[delete("/v0/pets/{name}")]
    #[api_v2_operation]
    fn delete_pet(_name: web::Path<String>) -> impl Future<Output = Result<web::Json<()>, ()>> {
        futures::future::ready(Ok(web::Json(())))
    }

    run_and_check_app(
        || {
            App::new()
                .wrap_api()
                .with_json_spec_at("/api/spec")
                .service(get_pets)
                .service(put_pet)
                .service(post_pet)
                .service(delete_pet)
                .build()
        },
        |addr| {
            let resp = CLIENT
                .get(&format!("http://{}/api/spec", addr))
                .send()
                .expect("request failed?");

            check_json(
                resp,
                json!({
                    "definitions": {
                        "Pet": {
                            "description": "Pets are awesome!",
                            "properties": {
                                "class": {
                                "enum": ["dog", "cat", "other"],
                                    "type":"string"
                                },
                                "id": {
                                    "format": "int64",
                                    "type": "integer"
                                },
                                "name": {
                                    "description": "Pick a good one.",
                                    "type": "string"
                                },
                                "birthday": {
                                  "format": "date",
                                  "type": "string"
                                },
                                "updatedOn": {
                                    "format": "date-time",
                                    "type": "string"
                                },
                                "uuid":{
                                    "format": "uuid",
                                    "type": "string"
                                }
                            },
                            "required":[
                                "birthday",
                                "class",
                                "name"
                            ],
                            "type":"object"
                        }
                    },
                    "info": {
                        "title":"",
                        "version":""
                    },
                    "paths": {
                        "/v0/pets": {
                            "get": {
                                "responses": {
                                    "200": {
                                        "description": "OK",
                                        "schema": {
                                            "items": {
                                                "$ref": "#/definitions/Pet"
                                            },
                                            "type": "array"
                                        }
                                    }
                                },
                            },
                            "post": {
                                "parameters": [
                                    {
                                        "in": "body",
                                        "name": "body",
                                        "required": true,
                                        "schema": {
                                            "$ref": "#/definitions/Pet"
                                        }
                                    }
                                ],
                                "responses": {
                                    "200": {
                                        "description": "OK",
                                        "schema": {
                                            "$ref": "#/definitions/Pet"
                                        }
                                    }
                                },
                            }
                        },
                        "/v0/pets/{name}": {
                            "delete": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                ],
                                "responses": {
                                    "200": {
                                        "description": "OK",
                                        "schema": {
                                        }
                                    }
                                },
                            },
                            "put": {
                                "parameters": [
                                    {
                                        "in": "path",
                                        "name": "name",
                                        "required": true,
                                        "type": "string"
                                    },
                                    {
                                        "in": "body",
                                        "name": "body",
                                        "required": true,
                                        "schema": {
                                            "$ref": "#/definitions/Pet"
                                        }
                                    }
                                ],
                                "responses": {
                                    "200": {
                                        "description": "OK",
                                        "schema": {
                                            "$ref": "#/definitions/Pet"
                                        }
                                    }
                                },
                            }
                        }
                    },
                    "swagger": "2.0"
                }),
            );
        },
    );
}

fn run_and_check_app<F, G, T, B, U>(factory: F, check: G) -> U
where
    F: Fn() -> App<T, B> + Clone + Send + Sync + 'static,
    B: MessageBody + 'static,
    T: ServiceFactory<
            Config = (),
            Request = ServiceRequest,
            Response = ServiceResponse<B>,
            Error = Error,
            InitError = (),
        > + 'static,
    G: Fn(String) -> U,
{
    let (tx, rx) = mpsc::channel();

    let _ = thread::spawn(move || {
        let sys = System::new("test");
        for port in 3000..30000 {
            if !PORTS.lock().insert(port) {
                continue;
            }

            let addr = format!("127.0.0.1:{}", port);
            let server = match HttpServer::new(factory.clone()).bind(&addr) {
                Ok(srv) => {
                    println!("Bound to {}", addr);
                    srv
                }
                Err(_) => continue,
            };

            let s = server.run();
            tx.send((s, addr)).unwrap();
            sys.run().expect("system error?");
            return;
        }

        unreachable!("No ports???");
    });

    let (_server, addr) = rx.recv().unwrap();
    let ret = check(addr);
    let _ = _server.stop(true);
    ret
}

fn check_json(resp: reqwest::blocking::Response, expected: serde_json::Value) {
    assert_eq!(resp.status().as_u16(), 200);
    let json = resp.json::<serde_json::Value>().expect("json error");

    if json != expected {
        panic!(
            "assertion failed:
  left: {}

 right: {}
",
            json.to_string(),
            expected.to_string()
        )
    }
}
