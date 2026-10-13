use poem::{
    Error,
    http::{Method, StatusCode},
    test::TestClient,
};
use poem_openapi::{
    ApiResponse, Object, OpenApi, OpenApiService,
    param::Query,
    payload::{Json, PlainText},
    registry::MetaApi,
    types::{MaybeUndefined, ToJSON},
};
use serde_json::Value;

#[tokio::test]
async fn query_explode_false() {
    #[derive(ApiResponse)]
    #[oai(bad_request_handler = "bad_request_handler")]
    enum MyResponse {
        /// Ok
        #[oai(status = 200)]
        Ok(Json<Option<Value>>),
        /// Bad Request
        #[oai(status = 400)]
        BadRequest(PlainText<String>),
    }

    fn bad_request_handler(err: Error) -> MyResponse {
        MyResponse::BadRequest(PlainText(format!("!!! {err}")))
    }

    const fn none<T>() -> MaybeUndefined<T> {
        MaybeUndefined::Undefined
    }

    struct Api;

    #[OpenApi]
    impl Api {
        #[oai(path = "/abc", method = "post")]
        async fn test(
            &self,
            #[oai(explode = false, default = "none::<Vec<u32>>")] fields: Query<
                MaybeUndefined<Vec<u32>>,
            >,
        ) -> MyResponse {
            MyResponse::Ok(Json(fields.0.to_json()))
        }
    }

    let meta: MetaApi = Api::meta().remove(0);
    assert_eq!(meta.paths[0].path, "/abc");
    assert_eq!(meta.paths[0].operations[0].method, Method::POST);

    let ep = OpenApiService::new(Api, "test", "1.0");
    let cli = TestClient::new(ep);

    let resp = cli.post("/abc").query("fields", &"1,2,3").send().await;
    resp.assert_status_is_ok();
    resp.assert_json(&[1, 2, 3]).await;

    let resp = cli.post("/abc").query("fields", &"").send().await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    let resp = cli.post("/abc").send().await;
    resp.assert_status_is_ok();
    resp.assert_json(Value::Null).await;
}

#[tokio::test]
async fn query_object_deep_object() {
    #[derive(Object, Debug, Eq, PartialEq)]
    #[oai(query)]
    struct Filter {
        name: String,
        age: u32,
        #[oai(default)]
        tags: Vec<String>,
        nickname: Option<String>,
    }

    struct Api;

    #[OpenApi]
    impl Api {
        #[oai(path = "/search", method = "get")]
        async fn search(&self, #[oai(style = "deep_object")] filter: Query<Filter>) -> Json<Value> {
            Json(filter.0.to_json().unwrap())
        }
    }

    // The object is emitted as a single query parameter using `deepObject` style.
    let meta: MetaApi = Api::meta().remove(0);
    let param = &meta.paths[0].operations[0].params[0];
    assert_eq!(param.name, "filter");
    assert_eq!(param.style, Some(poem_openapi::ParameterStyle::DeepObject));

    let ep = OpenApiService::new(Api, "test", "1.0");
    let cli = TestClient::new(ep);

    // Required + optional + array fields bind from `filter[...]` keys.
    let resp = cli
        .get("/search")
        .query("filter[name]", &"bob")
        .query("filter[age]", &"42")
        .query("filter[tags]", &"a")
        .query("filter[tags]", &"b")
        .query("filter[nickname]", &"bobby")
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({
        "name": "bob",
        "age": 42,
        "tags": ["a", "b"],
        "nickname": "bobby",
    }))
    .await;

    // Optional field omitted, defaulted array omitted.
    let resp = cli
        .get("/search")
        .query("filter[name]", &"alice")
        .query("filter[age]", &"30")
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({
        "name": "alice",
        "age": 30,
        "tags": [],
        "nickname": null,
    }))
    .await;

    // Missing required field -> bad request.
    let resp = cli.get("/search").query("filter[name]", &"x").send().await;
    resp.assert_status(StatusCode::BAD_REQUEST);

    // Invalid integer -> bad request.
    let resp = cli
        .get("/search")
        .query("filter[name]", &"x")
        .query("filter[age]", &"not-a-number")
        .send()
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn query_object_form() {
    #[derive(Object, Debug, Eq, PartialEq)]
    #[oai(query)]
    struct Pagination {
        page: u32,
        per_page: u32,
    }

    struct Api;

    #[OpenApi]
    impl Api {
        #[oai(path = "/list", method = "get")]
        async fn list(&self, page: Query<Pagination>) -> Json<Value> {
            Json(page.0.to_json().unwrap())
        }
    }

    // With the default (form) style, the object is exploded into one query
    // parameter per field instead of a single object parameter.
    let meta: MetaApi = Api::meta().remove(0);
    let params = &meta.paths[0].operations[0].params;
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "page");
    assert_eq!(
        params[0].in_type,
        poem_openapi::registry::MetaParamIn::Query
    );
    assert!(params[0].required);
    assert_eq!(params[0].style, None);
    assert_eq!(params[1].name, "per_page");
    assert!(params[1].required);

    let ep = OpenApiService::new(Api, "test", "1.0");

    // The spec exposes the two parameters directly (not a JSON object).
    let spec = ep.spec();
    assert!(spec.contains("\"name\": \"page\""));
    assert!(spec.contains("\"name\": \"per_page\""));

    let cli = TestClient::new(ep);

    // Fields are bound from top-level keys.
    let resp = cli
        .get("/list")
        .query("page", &"2")
        .query("per_page", &"25")
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({ "page": 2, "per_page": 25 }))
        .await;
}

#[tokio::test]
async fn query_object_list_form() {
    #[derive(Object, Debug, Eq, PartialEq)]
    #[oai(query)]
    struct MyData {
        people: Vec<String>,
    }

    struct Api;

    #[OpenApi]
    impl Api {
        #[oai(path = "/people", method = "get")]
        async fn people(&self, data: Query<MyData>) -> Json<Value> {
            Json(data.0.to_json().unwrap())
        }
    }

    // With the default (form) style the list field is exposed as a single
    // repeatable query parameter named after the field.
    let meta: MetaApi = Api::meta().remove(0);
    let params = &meta.paths[0].operations[0].params;
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "people");
    assert_eq!(
        params[0].in_type,
        poem_openapi::registry::MetaParamIn::Query
    );
    // `form` style with an array is exploded: repeated `people=...` keys.
    assert!(params[0].explode);
    assert_eq!(params[0].style, None);

    let ep = OpenApiService::new(Api, "test", "1.0");

    // The spec advertises the parameter as an array of strings.
    let spec = ep.spec();
    assert!(spec.contains("\"name\": \"people\""));
    assert!(spec.contains("\"type\": \"array\""));
    assert!(spec.contains("\"explode\": true"));

    let cli = TestClient::new(ep);

    // Each repeated `people` key becomes one element of the list, in order.
    let resp = cli
        .get("/people")
        .query("people", &"alice")
        .query("people", &"bob")
        .query("people", &"carol")
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({ "people": ["alice", "bob", "carol"] }))
        .await;

    // A single occurrence yields a one-element list.
    let resp = cli.get("/people").query("people", &"alice").send().await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({ "people": ["alice"] }))
        .await;

    // Omitting the field entirely yields an empty list.
    let resp = cli.get("/people").send().await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({ "people": [] })).await;

    // Comma-separated values are NOT split under the (default) exploded form
    // style: the whole string is a single element. Arrays must use repeated
    // keys instead.
    let resp = cli
        .get("/people")
        .query("people", &"alice,bob")
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({ "people": ["alice,bob"] }))
        .await;
}

#[tokio::test]
async fn query_object_list_deep_object() {
    #[derive(Object, Debug, Eq, PartialEq)]
    #[oai(query)]
    struct MyData {
        people: Vec<String>,
    }

    struct Api;

    #[OpenApi]
    impl Api {
        #[oai(path = "/people", method = "get")]
        async fn people(&self, #[oai(style = "deep_object")] data: Query<MyData>) -> Json<Value> {
            Json(data.0.to_json().unwrap())
        }
    }

    // With `deepObject` the whole struct stays a single object parameter and
    // list fields bind from repeated `data[people]=...` keys.
    let meta: MetaApi = Api::meta().remove(0);
    let param = &meta.paths[0].operations[0].params[0];
    assert_eq!(param.name, "data");
    assert_eq!(param.style, Some(poem_openapi::ParameterStyle::DeepObject));

    let ep = OpenApiService::new(Api, "test", "1.0");
    let cli = TestClient::new(ep);

    // Repeated `data[people]` keys populate the list, in order.
    let resp = cli
        .get("/people")
        .query("data[people]", &"alice")
        .query("data[people]", &"bob")
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({ "people": ["alice", "bob"] }))
        .await;

    // A single occurrence yields a one-element list.
    let resp = cli
        .get("/people")
        .query("data[people]", &"alice")
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({ "people": ["alice"] }))
        .await;

    // Omitting the field yields an empty list.
    let resp = cli.get("/people").send().await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({ "people": [] })).await;
}

#[tokio::test]
async fn query_object_flatten() {
    #[derive(Object, Debug, Eq, PartialEq)]
    #[oai(query)]
    struct Page {
        page: u32,
        per_page: u32,
    }

    #[derive(Object, Debug, Eq, PartialEq)]
    #[oai(query)]
    struct Query1 {
        name: String,
        #[oai(flatten)]
        page: Page,
    }

    struct Api;

    #[OpenApi]
    impl Api {
        #[oai(path = "/list", method = "get")]
        async fn list(&self, #[oai(style = "deep_object")] query: Query<Query1>) -> Json<Value> {
            Json(query.0.to_json().unwrap())
        }
    }

    let ep = OpenApiService::new(Api, "test", "1.0");
    let cli = TestClient::new(ep);

    // Flattened fields are bound at the same level as the parent's own fields.
    let resp = cli
        .get("/list")
        .query("query[name]", &"bob")
        .query("query[page]", &"1")
        .query("query[per_page]", &"10")
        .send()
        .await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({
        "name": "bob",
        "page": 1,
        "per_page": 10,
    }))
    .await;
}

#[tokio::test]
async fn query_object_explode_with_enum_and_skip() {
    use poem_openapi::Enum;

    #[derive(Enum, Debug, Eq, PartialEq)]
    #[oai(rename_all = "lowercase")]
    enum LogicOperator {
        And,
        Or,
    }

    #[derive(Debug, Eq, PartialEq)]
    struct FilterGroup(String);

    #[derive(Object, Debug, Eq, PartialEq)]
    #[oai(query)]
    struct PageQuery {
        #[oai(default)]
        page: u32,
        #[oai(default = "default_limit")]
        limit: u32,
        logic: LogicOperator,
        // Not a query parameter and not required to implement
        // `ParseFromParameter`.
        #[oai(skip)]
        group: Vec<FilterGroup>,
        dedup: Option<String>,
    }

    fn default_limit() -> u32 {
        20
    }

    struct Api;

    #[OpenApi]
    impl Api {
        #[oai(path = "/page", method = "get")]
        async fn page(&self, q: Query<PageQuery>) -> Json<Value> {
            Json(q.0.to_json().unwrap())
        }
    }

    // The skipped field is excluded; the remaining four fields become params.
    let meta: MetaApi = Api::meta().remove(0);
    let names: Vec<_> = meta.paths[0].operations[0]
        .params
        .iter()
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(names, ["page", "limit", "logic", "dedup"]);

    let ep = OpenApiService::new(Api, "test", "1.0");
    let cli = TestClient::new(ep);

    // Defaults apply for omitted fields; `logic` (required enum) is provided.
    let resp = cli.get("/page").query("logic", &"or").send().await;
    resp.assert_status_is_ok();
    resp.assert_json(serde_json::json!({
        "page": 0,
        "limit": 20,
        "logic": "or",
        "dedup": null,
    }))
    .await;
}

#[tokio::test]
async fn zzz_dump_spec() {
    #[derive(Object)]
    #[oai(query)]
    struct PageQuery {
        #[oai(default)]
        page: u32,
        limit: u32,
        dedup: Option<String>,
    }

    struct Api;

    #[OpenApi]
    impl Api {
        #[oai(path = "/page", method = "get")]
        async fn page(&self, q: Query<PageQuery>) -> Json<Value> {
            Json(q.0.dedup.into())
        }
    }

    let ep = OpenApiService::new(Api, "test", "1.0");
    println!("SPEC_START{}SPEC_END", ep.spec());
}
