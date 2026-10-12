# Typed client generation for poem-openapi

A summary of the investigation into generating a type-safe Rust client straight
from `poem-openapi` API definitions, without going Rust types → OpenAPI spec →
Rust types. It covers the decisions made along the way and the implementation plan.

Branch: `feat/openapi-client`

---

## 1. Goal

- Writing the server should be enough to get a typed client for it.
- The client should use the **same nominal types** as the server (`User`,
  `GetUserResponse`, …), not regenerated copies.
- An app that uses the client should take only a **light runtime
  dependency**, not the server's whole dependency tree.

## 2. Key findings

### Feasibility
- The `#[OpenApi]` macro already sees everything a client needs at compile
  time: the HTTP methods, the path template (`convert_oai_path`), each argument's
  name and type, and the response type (`poem-openapi-derive/src/api.rs`).
  The OpenAPI spec (`MetaApi`) is only a runtime view of this.
- A client is a third output of the macro, next to the existing
  `add_routes` (request decoding) and `meta()` (spec).

### Types are reused, not generated
- The generated client methods refer to the server's types through associated
  types, e.g. `<Json<User> as ClientArg>::Input` resolves to `User`. The client's
  return type is the server's own `ApiResponse` type. The only generated code is
  the client struct and its methods.
- `#[derive(Object)]`, `Enum` and `Union` already generate both `ToJSON` and
  `ParseFromJSON`, so DTOs work in both directions without changes.
- For reuse to work, DTOs must be at least as visible as the client, and
  validators also run when the client decodes a response.

### What is missing today
| Piece | Today | Needed |
|---|---|---|
| Request encoding | `ApiExtractor` only decodes | a `ClientArg` trait (`type Input`, encode into the request) |
| Parameter encoding | only `ParseFromParameter` (plus `ToHeader`) | a `ToParameter` trait with styles and `explode`; the MVP uses `ToJSON` for scalars |
| Response decoding | `ApiResponse` derive only emits `IntoResponse` | a `FromClientResponse` trait and its derive (status → variant, headers, body) |
| Payloads | server direction only | small client impls for each payload; multipart and SSE need more work |
| Transport | no HTTP client | `ClientTransport` (**done in Phase 0**) |

### `read_only` / `write_only`: decided to skip
- The server's `ParseFromJSON` **rejects** `read_only` fields that are present,
  and `ToJSON` **drops** `write_only` fields.
- We decided not to handle this; OpenAPI treats these flags as advisory.
- **Known limitation:** a client decoding a response that contains a `read_only`
  field (e.g. `id`) will get a decode error until this is revisited.
- If it is ever revisited: the client behaves as the exact mirror of the server
  (the two flags swapped). That can be added non-breakingly as default methods
  `parse_from_json_client` / `to_json_client`, with forwarding through every
  container type.

### The dependency problem and the decision
- Two crates share a nominal type only if both depend on the crate that
  **defines** it. A client generated from the server's `impl` block therefore
  drags in the whole server.
- **Rejected: build.rs codegen.** A build script can only see runtime metadata,
  which is essentially the spec. It has to generate look-alike types (losing
  nominal identity), is lossy (custom impls, generics, validators, `serialize_with`),
  and compiles the entire server, including sqlx, for every client build.
  Its one advantage is that a WASM client is easier.
- **Chosen: Option B, a trait-based interface crate.**
  - `my-api` (light): the DTOs plus the API as a `#[OpenApi] trait`. The macro
    generates the client from the trait.
  - `my-server` (heavy): `impl UserApi for Server`.
  - `my-app`: depends only on `my-api`.
  - Side benefit: server-only extractors like `Data<&Db>` stay out of the trait
    (state lives on `self`), so the client has no dummy `()` arguments.
- The original `my_service.client()` idea is superseded: the client is built
  from the interface crate and does not need a server instance.

### Trait support is additive (verified)
- The `#[OpenApi]` entry point (`poem-openapi-derive/src/lib.rs:108`) gets a
  second branch for traits; the `impl` path stays unchanged.
- One small refactor: `generate_operation` only reads `sig` and `attrs`, so it
  can take those two instead of an `ImplItemFn`.
- The runtime traits (`OpenApi`, `ApiExtractor`, `ApiResponse`) do not change.
- Two language rules, both tested with standalone programs:
  - A plain `async fn` in a trait does not give a `Send` future. The macro rewrites
    it to `fn … -> impl Future<Output = …> + Send`; the server can still use
    `async fn` in its `impl`.
  - The orphan rule (E0210) forbids `impl<T: UserApi> OpenApi for T`. The macro
    generates a wrapper `UserApiService<T>(pub T)` that implements `OpenApi`.
- What changes for users:
  - The server is wrapped when building the service:
    `OpenApiService::new(UserApiService(server), …)`.
  - Trait parameters must be plain identifiers (Rust forbids patterns in methods without a body).
  - `impl Future` in traits needs Rust 1.75; the repo MSRV is 1.85.

### Other caveats (later)
- `poem-openapi` depends on `poem` with its default `server` feature
  (hyper server, tokio net). That is much lighter than a real server, but making it
  optional is a separate follow-up.
- `#[oai(transform)]` can change the response on the wire, so the client can only
  make a best guess. `actual_type` should be used for decoding. WebSocket and static-file
  responses are not supported.

## 3. Status: Phase 0 done

- Features: `client` (also forwarded to `poem-openapi-derive/client`) and
  `client-reqwest`.
- `poem-openapi/src/client/`:
  - `ClientError`: `InvalidUrl`, `Transport`, `UnexpectedStatus { status, headers, body }`, `Decode`; `#[non_exhaustive]`.
  - `ClientTransport`: `send(Request) -> Result<Response, ClientError>`, with a
    blanket impl for any `poem::Endpoint` (in-process; errors become responses).
  - `ReqwestTransport`: bodies are buffered; TLS comes from the user's reqwest features.
  - `Client<Tr>`: base URL (absolute or `/`-prefixed) plus transport; `url()` and `send()`.
- Tests: unit tests (URL handling) and `tests/client_transport.rs`, which runs the same
  checks in-process and against a real TCP server via reqwest.

## 4. Implementation plan (MVP: `Path` parameters and responses)

**In MVP scope:**
- `Path<T>` with scalar `T`.
- Responses: `()`, `Json<T>`, `PlainText<String>`, `Result<T, E>`, and derived `ApiResponse`
  (fixed status, `status_range`, default variants, response headers).

**Deferred:** query, header, cookie and body parameters, security schemes, multipart,
SSE, XML/YAML, `transform`, `read_only`/`write_only`.

### Phase 0: Scaffolding ✅
See the status section above.

### Phase 1: Response decoding
1. Add a `FromClientResponse` trait:
   `async fn from_client_response(resp: poem::Response) -> Result<Self, ClientError>`.
2. Hand-written impls:
   - `()`
   - `Json<T: ParseFromJSON>`, which checks Content-Type with `Payload::check_content_type`
   - `PlainText<String>`
   - `Result<T, E>`, which picks a side using the existing `ApiResponse::meta()` status lists;
     undeclared statuses become `UnexpectedStatus`
3. `#[derive(ApiResponse)]` also emits `FromClientResponse` under `cfg(feature = "client")`:
   - matches fixed statuses, then ranges, then the default variant
   - decodes headers with `ParseFromParameter`
   - picks by Content-Type when two variants share a status, and errors at compile time if that is still ambiguous
4. Tests: property-style round trips, `from_client_response(x.into_response()) == x`.

### Phase 2: Path parameter encoding
1. `__private::encode_path_param<T: ToJSON>`: turns a scalar's JSON form into text and
   percent-encodes it as one segment. Arrays and objects are rejected.
2. Placeholders are filled **by position**, matching the server (`param0`, `param1`, …
   assigned to `Path` arguments in signature order).
3. Tests: encode, then `parse_from_parameter`, round-trips every scalar type, including values
   that need escaping (`/`, space, unicode, `%`).

### Phase 3: `#[OpenApi]` on traits (server side)
1. Add the trait branch in the macro entry point, and change `generate_operation` to take
   `(sig, attrs)`.
2. Rewrite `async fn` → `fn … -> impl Future<Output = R> + Send`, and add `Send + Sync + 'static`
   bounds.
3. Generate the `{Trait}Service<T>(pub T)` adapter with `impl<T: Trait> OpenApi`,
   reusing the existing route, meta and register code.
4. Compile errors for: default method bodies, pattern parameters, non-`async` methods.
5. Tests:
   - The existing API test cases, in trait form.
   - **Spec equality:** the same API defined as an `impl` and as a trait produces an identical spec.
   - A hygiene test in `tests/hygiene.rs`.

### Phase 4: Client generation from traits
1. **Spike first:** in a test, hand-write the `{Trait}Client<Tr>` the macro would
   generate, and make it pass against the server. This fixes the shape of
   `ClientArg` (`type Input`, encode) before any macro work.
2. The macro emits `pub struct {Trait}Client<Tr>` (wrapping `Client<Tr>`), one method per
   trait method: arguments of type `<ArgTy as ClientArg>::Input`, return type
   `Result<ResTy, ClientError>` (or `actual_type`).
3. Generated only with the `client` feature. Add `#[OpenApi(skip_client)]` as an escape
   hatch while not every argument type is supported.
4. Decisions:
   - Several HTTP methods on one operation: use the first one.
   - `hidden` operations: still generated.
   - The client inherits the trait's visibility.
   - `prefix_path` is evaluated at runtime.
5. Tests: replace the spike with the macro output; the spike tests must pass unchanged.

### Phase 5: End-to-end tests
Every case runs over **both** transports: in-process `Endpoint`, and a real TCP server
(`127.0.0.1:0`) via reqwest.

**Cases:**
- no arguments and a `()` response
- one and several `Path` arguments, in and out of URL order
- escaping
- every scalar type and a derived `Enum`
- `prefix_path`
- a service nested under `Route::nest("/api", …)`, with the matching base URL
- `ApiResponse`: several statuses, headers, `status_range`, default
- `Result` with both `Ok` and `Err`
- an undeclared status gives `UnexpectedStatus` with the body kept
- a wrong Content-Type gives a decode error
- a generic trait
- **type identity:** assigning the client result to a variable of the server's response type

**Dependency split:** an example workspace with three crates (`api`, `server`, `app`).
Check that `cargo tree -p app` contains no server-only dependencies.

### Phase 6: Docs and example
- `examples/openapi/client` with the three-crate layout.
- A client section in `poem-openapi/src/docs/openapi.md`.
- A list of limitations.

### Suggested PRs
1. Phases 0–2 (runtime, transports, response decoding, path encoding).
2. Phase 3 (trait support on its own; useful even without the client).
3. Phases 4–6 (client generation, e2e tests, docs).

### After the MVP
- `Query`, `Header` and `Json` bodies with a real `ToParameter` trait.
- Security schemes: credentials set once on the client.
- Multipart encoding and an SSE client stream.
- Making `poem-openapi`'s dependency on `poem`'s `server` feature optional (light/WASM clients).
- Revisiting `read_only`/`write_only` if decode errors become a problem.

## 5. Development notes

- Run tests with `flox activate -- just claude-test [cargo test args]`.
  The rustup toolchain cached in `.flox/cache/rustup` has a stale path
  (`/home/max/oss/poem`) in its rust-lld wrapper. The recipe sets
  `RUSTFLAGS`/`RUSTDOCFLAGS=-Clinker-features=-lld` to use the system linker.
  The lasting fix is to reinstall the toolchain inside the flox environment.
- Clippy 1.97 reports two errors in files that were not changed:
  `poem-openapi/tests/multipart.rs:488` and
  `poem-openapi-derive/src/security_scheme.rs:247`.
- `cargo fmt` with this toolchain reformats unrelated files. Revert those so diffs stay focused.

---

## 6. Idea under consideration: a separate crate instead of changing poem-openapi

Instead of adding trait support and client generation to `poem-openapi`, build a **new
crate** that becomes the home of the shared API definition. You write the routes once, as a
trait, in your shared `my-api` crate, and the new crate's macro generates both the server
and the client.

### Why it works
The new macro emits a normal poem-openapi `impl` block that just forwards each call, so
poem-openapi's own, unchanged macro generates the server:

```rust
// You write this once, in the shared `my-api` crate:
#[poem_api::api]                      // the new crate's macro
pub trait UserApi {
    #[oai(path = "/users/:id", method = "get")]
    async fn get_user(&self, id: Path<i64>) -> GetUserResponse;
}

// The macro generates:
pub struct UserApiService<T>(pub T);

#[poem_openapi::OpenApi]              // the existing, unchanged macro
impl<T: UserApi> UserApiService<T> {
    #[oai(path = "/users/:id", method = "get")]   // attributes copied through as they are
    async fn get_user(&self, id: Path<i64>) -> GetUserResponse {
        self.0.get_user(id).await
    }
}

pub struct UserApiClient<Tr> { /* … */ }  // plus the client
```

poem-openapi already supports and tests this generic wrapper pattern
(`poem-openapi/tests/api.rs:698`). Every server feature and attribute keeps working, with
zero changes to poem-openapi, and Phase 3 becomes simpler (no second macro branch).

### What the new crate would contain
- The trait macro, which generates the forwarding `impl` and the client.
- The client runtime: the Phase 0 code (`Client`, transports, `ClientError`) moves here.
- The `ClientArg` and `FromClientResponse` traits, implemented for poem-openapi's `Path`,
  `Json` and so on. Rust allows this because the traits are ours.
- A derive for decoding responses.

### Trade-offs
- ✅ poem-openapi stays untouched, and its maintainers don't need to agree.
- ✅ The crate ships and changes at its own pace, and could be upstreamed later.
- ✅ Server behaviour can't drift, because poem-openapi itself generates the server.
- ❌ Response types need one extra derive: `#[derive(ApiResponse, ClientResponse)]`. It
  reads the same `#[oai(...)]` attributes, so nothing else is repeated.
- ❌ The client has to copy some of poem-openapi's rules (path templating, path parameters
  matched by position, status → variant). Those could drift if poem-openapi changes them;
  pin the poem-openapi version and let the end-to-end tests catch it.
- ❌ A few useful helpers are semi-internal (`registry` is `#[doc(hidden)]`, `join_path` is in
  `__private`), so there are no stability guarantees. Use them as little as possible or copy the small ones.
- ➖ Two layers of macros make compile errors harder to read. Keeping the original spans helps.

### What would change in the plan
- **Phase 0:** move `client/` and its tests into the new crate, and revert the small changes to
  poem-openapi's `Cargo.toml` and `lib.rs`.
- **Phase 1:** the response derive becomes the new crate's `ClientResponse` derive.
- **Phase 3:** now just "the trait macro emits the forwarding `impl`"; the rewrite of `async fn`
  for `Send` stays.
- **Phases 4–6:** unchanged.

### Open decisions
- Where the crate lives: in this workspace (easier to test against poem-openapi) or in its own repo.
- Its name, e.g. `poem-openapi-client` or `poem-api`.
