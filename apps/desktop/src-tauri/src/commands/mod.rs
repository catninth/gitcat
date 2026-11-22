//! The commands the webview invokes, one module per area. Each one is a thin
//! adapter onto `CoreApi` or a hosting service; the behaviour lives behind it.

pub(crate) mod branches;
pub(crate) mod commits;
pub(crate) mod conflicts;
pub(crate) mod forge;
pub(crate) mod history;
pub(crate) mod remotes;
pub(crate) mod repository;
pub(crate) mod settings;
pub(crate) mod stash;
pub(crate) mod submodules;
pub(crate) mod worktree;
