use std::fmt;

/// The file of an app a problem is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AppFile {
    /// The folder itself: files that are missing or do not belong.
    Folder,
    Manifest,
    Compose,
    Icon,
}

impl AppFile {
    /// The file name, or `None` for the folder.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::Folder => None,
            Self::Manifest => Some(crate::MANIFEST_FILE),
            Self::Compose => Some(crate::compose::COMPOSE_FILE),
            Self::Icon => Some(crate::ICON_FILE),
        }
    }
}

/// Something about an app that keeps it out of the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub file: AppFile,
    /// Where in the file, as a path such as `services.web.ports[0]`; empty
    /// when the problem is about the file as a whole.
    pub at: String,
    /// The line, when it is known (syntax errors).
    pub line: Option<u64>,
    pub message: String,
}

impl Problem {
    pub fn new(file: AppFile, at: impl Into<String>, message: impl Into<String>) -> Self {
        Self { file, at: at.into(), line: None, message: message.into() }
    }

    pub fn folder(message: impl Into<String>) -> Self {
        Self::new(AppFile::Folder, "", message)
    }

    pub fn manifest(at: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AppFile::Manifest, at, message)
    }

    pub fn compose(at: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(AppFile::Compose, at, message)
    }

    pub fn icon(message: impl Into<String>) -> Self {
        Self::new(AppFile::Icon, "", message)
    }

    pub(crate) fn at_line(mut self, line: Option<u64>) -> Self {
        self.line = line;
        self
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.file.name() {
            write!(f, "{name}")?;
            if let Some(line) = self.line {
                write!(f, ":{line}")?;
            }
            write!(f, ": ")?;
        }
        if !self.at.is_empty() {
            write!(f, "{}: ", self.at)?;
        }
        write!(f, "{}", self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_file_line_and_place() {
        let problem = Problem::compose("services.web.privileged", "no").at_line(Some(3));
        assert_eq!(problem.to_string(), "compose.yaml:3: services.web.privileged: no");
        assert_eq!(Problem::folder("`x` is missing").to_string(), "`x` is missing");
    }
}
