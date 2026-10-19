use std::path::{Path, PathBuf};

use oxframe_2d::View;
use oxframe_core::BoxFuture;

/// Where a scene was defined; used to find its `<name>.meta.json`.
#[derive(Debug, Clone, Copy)]
pub struct SourceLocation {
    /// `file!()` of the scene, relative to the workspace or package root.
    pub file: &'static str,
    /// `CARGO_MANIFEST_DIR` of the crate that defined the scene.
    pub manifest_dir: &'static str,
}

/// A scene produced by `#[oxframe::scene]`.
#[derive(Clone, Copy)]
pub struct SceneDef {
    pub(crate) name: &'static str,
    pub(crate) location: SourceLocation,
    pub(crate) factory: fn(View) -> BoxFuture,
}

impl std::fmt::Debug for SceneDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneDef")
            .field("name", &self.name)
            .field("location", &self.location)
            .finish()
    }
}

impl SceneDef {
    /// Used by `#[oxframe::scene]`; you normally don't call it.
    pub fn new(
        name: &'static str,
        location: SourceLocation,
        factory: fn(View) -> BoxFuture,
    ) -> Self {
        Self {
            name,
            location,
            factory,
        }
    }

    /// Scene name (the function name).
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// `<dir of the source file>/<name>.meta.json`.
    ///
    /// `file!()` is relative to the workspace root for workspace members and
    /// to the package root otherwise, so the manifest dir and its ancestors
    /// are tried in turn.
    pub fn meta_path(&self) -> PathBuf {
        let file_name = format!("{}.meta.json", self.name);
        let manifest = Path::new(self.location.manifest_dir);
        let source = Path::new(self.location.file);
        let dir = manifest
            .ancestors()
            .map(|base| base.join(source))
            .find(|p| p.is_file())
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| manifest.to_path_buf());
        dir.join(file_name)
    }
}
