use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use rspack::builder::{Builder, Devtool};
use rspack_core::{
    Compiler, ModuleOptions, ModuleRule, ModuleRuleEffect, ModuleRuleUse, ModuleRuleUseLoader,
    OutputOptions, Resolve, RuleSetCondition, TsconfigOptions, TsconfigReferences,
};
use rspack_fs::MemoryFileSystem;
use rspack_plugin_html::{HtmlRspackPlugin, config::HtmlRspackPluginOptions};
use rspack_regex::RspackRegex;
use std::{env, fs, sync::Arc};

use crate::watcher::Watching;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    StatusCode(StatusCode),

    #[error(transparent)]
    InvalidHeaderValue(#[from] axum::http::header::InvalidHeaderValue),
}

impl From<StatusCode> for Error {
    #[inline]
    fn from(code: StatusCode) -> Self {
        Error::StatusCode(code)
    }
}

impl IntoResponse for Error {
    #[inline]
    fn into_response(self) -> Response {
        match self {
            Error::StatusCode(code) => match code.canonical_reason() {
                Some(reason) => (code, reason).into_response(),
                None => code.into_response(),
            },
            #[cfg(debug_assertions)]
            error => (StatusCode::INTERNAL_SERVER_ERROR, format!("{:#?}", error)).into_response(),
            #[cfg(not(debug_assertions))]
            error => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

async fn get_index(State(watching): State<Watching>) -> Result<(HeaderMap, Vec<u8>), Error> {
    if let Some((mime_type, content)) = watching.get_asset("index.html").await {
        Ok((
            HeaderMap::from_iter([(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime_type.as_ref())?,
            )]),
            content,
        ))
    } else {
        Err(StatusCode::NOT_FOUND)?
    }
}

async fn get_asset(
    State(watching): State<Watching>,
    Path(path): Path<String>,
) -> Result<(HeaderMap, Vec<u8>), Error> {
    if let Some((mime_type, content)) = watching.get_asset(path).await {
        Ok((
            HeaderMap::from_iter([(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime_type.as_ref())?,
            )]),
            content,
        ))
    } else {
        Err(StatusCode::NOT_FOUND)?
    }
}

pub fn routes() -> Router {
    let compiler = Compiler::builder()
        .mode("development".into())
        .devtool(Devtool::InlineSourceMap)
        .context(env!("CARGO_MANIFEST_DIR"))
        .entry("main", "./frontend/index.ts")
        .output(OutputOptions::builder().path("/"))
        .resolve(Resolve {
            tsconfig: Some(TsconfigOptions {
                config_file: "./tsconfig.json".into(),
                references: TsconfigReferences::Auto,
            }),
            ..Default::default()
        })
        .module(ModuleOptions {
            rules: vec![ModuleRule {
                test: Some(RuleSetCondition::Regexp(
                    RspackRegex::new("\\.ts$").unwrap(),
                )),
                effect: ModuleRuleEffect {
                    r#use: ModuleRuleUse::Array(vec![ModuleRuleUseLoader {
                        loader: "builtin:swc-loader".to_string(),
                        options: Some(fs::read_to_string(".swcrc").unwrap()),
                    }]),
                    ..Default::default()
                },
                ..Default::default()
            }],
            ..Default::default()
        })
        .plugin(Box::new(HtmlRspackPlugin::new(
            HtmlRspackPluginOptions::default(),
        )))
        .output_filesystem(Arc::new(MemoryFileSystem::default()))
        .enable_loader_swc()
        .build()
        .unwrap();
    let watching = Watching::new(compiler, None, None);

    Router::new()
        .route("/", axum::routing::get(get_index))
        .route("/{*path}", axum::routing::get(get_asset))
        .with_state(watching)
}
