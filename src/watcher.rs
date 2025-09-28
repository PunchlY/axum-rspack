use mime_guess::Mime;
use rspack_core::Compiler;
use rspack_fs::{
    EventAggregateHandler, EventHandler, FsWatcher, FsWatcherIgnored, FsWatcherOptions,
};
use rspack_paths::Utf8Path;
use rspack_regex::RspackRegex;
use rspack_util::fx_hash::FxHashSet;
use std::{collections::HashSet, sync::Arc, time::SystemTime};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Watching {
    compiler: Arc<RwLock<Compiler>>,
    watcher: Arc<RwLock<FsWatcher>>,
}

impl Watching {
    pub fn new(
        compiler: Compiler,
        options: Option<FsWatcherOptions>,
        ignored: Option<FsWatcherIgnored>,
    ) -> Self {
        let compiler = Arc::new(RwLock::new(compiler));

        let watcher = FsWatcher::new(
            options.unwrap_or(FsWatcherOptions {
                follow_symlinks: false,
                poll_interval: None,
                aggregate_timeout: None,
            }),
            ignored.unwrap_or(FsWatcherIgnored::Regex(
                RspackRegex::new(r#"[\/](?:\.git|node_modules)[\/]"#).unwrap(),
            )),
        );
        let watcher = Arc::new(RwLock::new(watcher));

        let watching = Self { compiler, watcher };

        tokio::spawn({
            let watching = watching.clone();
            async move { watching.build().await }
        });

        watching
    }

    pub async fn build(&self) {
        let start_time = SystemTime::now();
        self.watcher.read().await.pause().unwrap();

        self.compiler.write().await.build().await.ok();

        let compiler = self.compiler.read().await;
        let files = compiler.compilation.file_dependencies();
        let missing = compiler.compilation.missing_dependencies();

        self.watcher
            .write()
            .await
            .watch(
                (files.0.cloned(), files.2.cloned()),
                (std::iter::empty(), std::iter::empty()),
                (missing.0.cloned(), missing.2.cloned()),
                start_time,
                Box::new(self.clone()),
                Box::new(self.clone()),
            )
            .await;
    }

    pub async fn rebuild(&self, changed_files: HashSet<String>, deleted_files: HashSet<String>) {
        let start_time = SystemTime::now();
        self.watcher.read().await.pause().unwrap();

        self.compiler
            .write()
            .await
            .rebuild(changed_files, deleted_files)
            .await
            .ok();

        let compiler = self.compiler.read().await;
        let files = compiler.compilation.file_dependencies();
        let missing = compiler.compilation.missing_dependencies();

        for diagnostic in compiler.compilation.get_errors() {
            tracing::warn!("{:?}", diagnostic);
        }

        self.watcher
            .write()
            .await
            .watch(
                (files.1.cloned(), files.2.cloned()),
                (std::iter::empty(), std::iter::empty()),
                (missing.1.cloned(), missing.2.cloned()),
                start_time,
                Box::new(self.clone()),
                Box::new(self.clone()),
            )
            .await;
    }

    pub async fn get_asset(&self, path: impl AsRef<Utf8Path>) -> Option<(Mime, Vec<u8>)> {
        let compiler = self.compiler.read().await;
        let path = compiler.options.output.path.join(path);
        let fs = &compiler.compilation.output_filesystem;
        if let Ok(metadata) = fs.stat(&path).await
            && metadata.is_file
        {
            let content = fs.read_file(&path).await.unwrap();
            let mime_type = mime_guess::from_path(&path).first_or_octet_stream();
            Some((mime_type, content))
        } else {
            None
        }
    }
}

impl EventAggregateHandler for Watching {
    fn on_event_handle(&self, changed_files: FxHashSet<String>, deleted_files: FxHashSet<String>) {
        let compiler = self.clone();
        tracing::warn!(?changed_files, ?deleted_files);
        tokio::spawn(async move {
            let _ = compiler
                .rebuild(
                    changed_files.into_iter().collect::<HashSet<_>>(),
                    deleted_files.into_iter().collect::<HashSet<_>>(),
                )
                .await;
        });
    }
}

impl EventHandler for Watching {}
