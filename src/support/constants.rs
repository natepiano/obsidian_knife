// macos file dates
#[cfg(target_os = "macos")]
pub(super) const SET_FILE_CREATED_DATE_FLAG: &str = "-d";
#[cfg(target_os = "macos")]
pub(super) const SET_FILE_CREATED_DATE_FORMAT: &str = "%m/%d/%Y %H:%M:%S";
#[cfg(target_os = "macos")]
pub(super) const SET_FILE_EXECUTABLE: &str = "SetFile";
