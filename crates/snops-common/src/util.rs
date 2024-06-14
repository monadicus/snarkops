use std::fmt::Debug;

/// A wrapper struct that has an "opaque" `Debug` implementation for types
/// that do not implement `Debug`.
pub struct OpaqueDebug<T>(pub T);

impl<T> Debug for OpaqueDebug<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("(...)")
    }
}

impl<T> std::ops::Deref for OpaqueDebug<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for OpaqueDebug<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
