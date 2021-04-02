# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Add support for actix-web-macros methods routing [PR#289](https://github.com/wafflespeanut/paperclip/pull/289)
- Add support for actix-web 4

### Changed

### Fixed
- Optional type aliases like `type Email = Option<String>` will not be added to the `required` fields.
- Actix plugin: Path tuples now inherit field names and descriptions from doc comments

## [0.5.0] - 2020-11-28
### Added
- Actix plugin: Support for actix-web 3.0 (is now default).
- Arrays up to length 32 are supported (in codegen and actix-web plugin).
- Actix plugin: `#[api_v2_operation]` macro now supports specifying `consumes`, `produces`, `summary`, `description`, `tags`
and `operation_id` in macro.
- Actix plugin: Support for `actix-session`, `serde_qs` and `chrono` types in handlers.
- Actix plugin: `App::wrap_api_with_spec` allows to provide default specification with `info` and other custom settings
- Actix plugin: Support tags in api_v2_operation macros.
- Actix plugin: `#[serde(flatten)]` support in `Apiv2Schema` derive.
- Actix plugin: Added wrapper types for some 2xx status codes.

### Changed
- Actix plugin: Refactored internals of `#[api_v2_operation]` proc macro (long-outstanding technical debt). This now generates operation metadata (on the fly) for each handler, which enables us to tie custom changes to operations easily.
- Actix plugin: Grouping of parameters across handlers have been disabled as a result of major bugs (it's now under `normalize` feature).
- Actix plugin: actix-web `2.x` is supported through `actix2` and `actix2-nightly` features.

### Fixed
- `Apiv2Schema` supports `HashMap<Uuid, Foo>`.
- Actix plugin: `#[api_v2_operation]` supports referencing inside handlers.
- Actix plugin: Fixed a bug on using `where` clause in handlers marked with `#[api_v2_operation]`.
- Actix plugin: API objects have `object` type specified in their schemas.

## [0.4.1] - 2020-07-01
### Fixed
- Re-exported import from `proc-macro-error` which broke builds.

## [0.4.0] - 2020-06-13
### Added
- Client timeout in CLI.
- Codegen for header parameters in operations.
- Validation for non-body parameters.
- `x-rust-dependencies` field in root for specifying additional crate dependencies.
- Default en/decoders (JSON/YAML) and support for custom en/decoders through `x-rust-coders` field in root.
- Methods to `Schema` trait for aiding resolution and codegen.
- Codegen for nested arrays in operation parameters.
- Codegen for form data parameters in operations.
- Codegen for `Any` type in schema.
- CLI payload encoding/decoding supports custom types and not limited to JSON.
- Codegen for `#[deprecated]` attribute when `deprecated` field is set to `true` in schema.
- Codegen adds `Request` and `Response` traits for HTTP request and response objects.
- CLI uses name and version from `info` field in spec.
- Codegen for file responses with streaming to `AsyncWrite` implementors.
- Codegen for `multipart/form-data` parameters with file streaming.
- Referencing globally defined parameters and responses.
- Codegen for enums in object definitions.
- Response wrapper containing headers and status code for operation.
- Parsing for custom response headers.
- Actix plugin: `#[openapi(empty)]` attribute can be used to any type to implement empty schema and ignore the warning.
- Actix plugin: Empty impls for some actix-web types (like `Payload`, `Data<T>`, etc.).
- Actix plugin: Raw JSON spec generation from handlers.
- Actix plugin: Support for `#[serde(rename = "...")]` and `#[serde(rename_all = "...")]`.
- Actix plugin: Support for error (non-2xx) response codes.
- Actix plugin: Type-level and field-level documentation is now used for `description` fields in schema and properties.
- Actix plugin: Security definitions (globally) and security requirements (for operations).
- Actix plugin: Support for `serde_json::Value` and `serde_yaml::Value` (by default), and for objects from `uuid`, `rust_decimal`, `chrono`, etc. (through features of the same name) in structs.

### Changed
- Switched to templating for (almost) static modules.
- Operation IDs are preferred for method names if they exist.
- `ApiClient` uses the newly added `Request` and `Response` and is now async/await.
- `Sendable` accepts the new `ApiClient` and is now async/await.
- `SchemaRepr` renamed to `Resolvable`.
- `OperationMap` renamed to `PathItem`.
- `Api` struct is now generic over parameters and responses in addition to definitions.
- Actix plugin: Switched to `actix-web = "^2.0"`.
- Actix plugin: **Supports stable compiler**.
- Actix plugin: `#[api_v2_schema]` macro attribute is now `#[derive(Apiv2Schema)]`.

### Fixed
- Switched `enum` field to array of `any` rather than strings.
- Resolution of anonymous schema definitions in objects, operation parameters and responses.
- Unmappable operations (i.e., without body parameters and simple response types) are now namespaced in a separate module.
- Array definitions are now allowed in schemas.
- `additionalProperties` takes boolean or a schema.
- Deadlock when resolving some recursive types.
- Actix plugin: `.route()` method call on `App`, `Scope` and `ServiceConfig` don't override existing route operations.
- Actix plugin: `web::Path<T>` also supports simple types (strings, integers, etc.).
- Actix plugin: `#[api_v2_schema]` derivatives can now use references.
- Actix plugin: Breakage of `#[api_v2_operation]` when returning `impl Handler`.
- Actix plugin: `web::scope` supports having path parameters.
- Actix plugin: Misuse of `actix_web::Scope` in `App::configure` which resulted in missing overwritten routes.

## [0.3.0] - 2019-07-30
### Added
- Paperclip's `#[api_v2_schema]` derives raw schema structs (i.e., no smart pointers) along with actual schema structs for other users and plugins. An effect of this would be that now there's a `DefaultSchemaRaw` in addition to `DefaultSchema`.
- `TypedData`, `Apiv2Schema` and `Apiv2Operation` traits for deriving v2 spec for server plugins.
- `paperclip-core` crate for segregating core types and traits.
- `paperclip-actix` crate as an actix-web plugin for hosting v2 spec as a JSON by marking models and operations using proc macro attributes.
- `paperclip::actix` module for exporting `paperclip_actix::*` when `actix` feature is enabled.

### Changed
- Segregated dependencies using feature gates.
- `Api::<S>::resolve` now returns `ValidationError` instead of `failure::Error`.
- During serialization, optional schema fields are skipped when empty/null.

### Fixed
- Allowing ports in `host` field in v2 spec.

## [0.2.0] - 2019-07-03
### Added
- Gitbook for detailed documentation and walkthroughs.
- Changelog
- Root module (`mod.rs`, `lib.rs` or `main.rs`) generation for codegen (previously we were only generating children modules).
- Cargo manifest generation (gated by `"cli"` feature).
- `[bin]` target (CLI) for generating crates.
- CLI generation (fancy curl for your APIs) - generated app uses async/await and `runtime_tokio`.
- `ApiClient::make_request` for sending a request and fetching a response future.
- Support for operations returning array of objects.
- Codegen uses `basePath` and `host` fields (if they exist) to override default base URL (`https://example.com`).
- API relative paths are checked for uniqueness.

### Changed
- Codegen now writes the dependency traits, types and impls in the root module.

### Removed
- `ApiClient::base_url` in favor of `ApiClient::make_request`
- Redundant `Optional` trait for generated markers.

### Fixed
- Templated paths are now validated against parameters.
- Import prefixes support in emitter (for codegen).

## [0.1.0] - 2019-06-13
### Added
- Build script example in README.
- `impl Sendable` for fulfilled builders to send API requests and return response futures.
- Documentation for builders and methods from `description` fields.
- Generation of `#[repr(transparent)]` builders with phantom fields for enforcing required parameters.
- Generation of builders for API objects and operations.
- Generation of Rust structs with appropriate serde rules from definitions.
- `#[api_schema]` proc-macro attribute for implementing `Schema` trait for custom schemas.
- Resolution of `$ref` references in definitions and paths to objects in the same file.
- Loading OpenAPI v2 schema from JSON/YAML
- Workspace, README, LICENSE, Makefile, CI config, etc.

[Unreleased]: https://github.com/wafflespeanut/paperclip/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/wafflespeanut/paperclip/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/wafflespeanut/paperclip/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/wafflespeanut/paperclip/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/wafflespeanut/paperclip/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/wafflespeanut/paperclip/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/wafflespeanut/paperclip/releases/tag/v0.1.0
