// Ported from baritone src/api/java/baritone/api/utils/PathCalculationResult.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use std::fmt;

use crate::api::pathing::calc::IPath;

pub struct PathCalculationResult {
    path: Option<Box<dyn IPath>>,
    type_: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    SuccessToGoal,
    SuccessSegment,
    Failure,
    Cancellation,
    Exception,
}

impl PathCalculationResult {
    /// `PathCalculationResult(Type)`
    pub fn new(type_: Type) -> Self {
        Self { path: None, type_ }
    }

    /// `PathCalculationResult(Type, IPath)`
    pub fn with_path(type_: Type, path: Box<dyn IPath>) -> Self {
        Self {
            path: Some(path),
            type_,
        }
    }

    pub fn get_path(&self) -> Option<&dyn IPath> {
        self.path.as_deref()
    }

    /// Takes the path out of the result.
    pub fn into_path(self) -> Option<Box<dyn IPath>> {
        self.path
    }

    pub fn get_type(&self) -> Type {
        self.type_
    }
}

impl fmt::Debug for PathCalculationResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PathCalculationResult")
            .field("type", &self.type_)
            .field("path", &self.path)
            .finish()
    }
}
