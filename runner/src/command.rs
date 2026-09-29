#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum Command {
    Algorithm,
    Execute,
    Metadata,
    Preferences,
    Signals,
}
