use std::ops::{Deref, DerefMut};

use poem::{Request, RequestBody, Result};

use crate::{
    ApiExtractor, ApiExtractorType, ExtractParamOptions,
    base::UrlQuery,
    error::ParseParamError,
    registry::{MetaParamIn, MetaSchemaRef, Registry},
    types::ParseFromParameter,
};

/// Represents the parameters passed by the query string.
pub struct Query<T>(pub T);

impl<T> Deref for Query<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Query<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a, T: ParseFromParameter> ApiExtractor<'a> for Query<T> {
    const TYPES: &'static [ApiExtractorType] = &[ApiExtractorType::Parameter];
    const PARAM_IS_REQUIRED: bool = T::IS_REQUIRED;

    type ParamType = T;
    type ParamRawType = T::RawValueType;

    fn register(registry: &mut Registry) {
        T::register(registry);
    }

    fn param_in() -> Option<MetaParamIn> {
        Some(MetaParamIn::Query)
    }

    fn param_schema_ref() -> Option<MetaSchemaRef> {
        Some(T::schema_ref())
    }

    fn __query_parameters(
        explode: bool,
        style: Option<crate::ParameterStyle>,
    ) -> Option<Vec<crate::registry::MetaOperationParam>> {
        T::__query_object_fields(explode, style)
    }

    fn param_raw_type(&self) -> Option<&Self::ParamRawType> {
        self.0.as_raw_value()
    }

    async fn from_request(
        request: &'a Request,
        _body: &mut RequestBody,
        param_opts: ExtractParamOptions<Self::ParamType>,
    ) -> Result<Self> {
        let url_query = request.extensions().get::<UrlQuery>().unwrap();

        if let Some(default_value) = &param_opts.default_value {
            if url_query
                .get_all_case(param_opts.name, param_opts.ignore_case)
                .is_empty()
            {
                return Ok(Self(default_value()));
            }
        }

        T::parse_from_query(url_query, &param_opts)
            .map(Self)
            .map_err(|err| {
                ParseParamError {
                    name: param_opts.name,
                    reason: err.into_message(),
                }
                .into()
            })
    }
}
