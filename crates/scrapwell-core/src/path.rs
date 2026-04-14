use std::path::{Path, PathBuf};

use crate::error::ScrapwellError;

/// Represents a document's location: entity / (topic) / name
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MemoryPath {
    pub entity: String,
    pub topic: Option<String>,
    pub name: String,
}

impl MemoryPath {
    pub fn new(entity: &str, topic: Option<&str>, name: &str) -> Result<Self, ScrapwellError> {
        validate_segment(entity)?;
        if let Some(t) = topic {
            validate_segment(t)?;
        }
        validate_segment(name)?;

        Ok(Self {
            entity: entity.to_string(),
            topic: topic.map(|t| t.to_string()),
            name: name.to_string(),
        })
    }

    /// Convert to filesystem path relative to root.
    /// e.g. root/entities/elasticsearch/mapping/nested-dense-vector.md
    pub fn to_fs_path(&self, root: &Path) -> PathBuf {
        let mut path = root.join("entities").join(&self.entity);
        if let Some(ref topic) = self.topic {
            path = path.join(topic);
        }
        path.join(format!("{}.md", self.name))
    }

    /// Path to the _entity.md file.
    pub fn entity_dir(&self, root: &Path) -> PathBuf {
        root.join("entities").join(&self.entity)
    }
}

impl std::fmt::Display for MemoryPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.topic {
            Some(topic) => write!(f, "{}/{}/{}", self.entity, topic, self.name),
            None => write!(f, "{}/{}", self.entity, self.name),
        }
    }
}

fn validate_segment(s: &str) -> Result<(), ScrapwellError> {
    if s.is_empty() {
        return Err(ScrapwellError::InvalidPath("empty segment".to_string()));
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(ScrapwellError::InvalidPath(format!(
            "invalid segment '{}': only lowercase ASCII, digits, and hyphens allowed",
            s
        )));
    }
    Ok(())
}

/// Validate an entity name.
pub fn validate_entity_name(name: &str) -> Result<(), ScrapwellError> {
    validate_segment(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_path_without_topic() {
        let p = MemoryPath::new("rust", None, "anyhow-vs-thiserror").unwrap();
        assert_eq!(p.entity, "rust");
        assert_eq!(p.topic, None);
        assert_eq!(p.name, "anyhow-vs-thiserror");
        assert_eq!(p.to_string(), "rust/anyhow-vs-thiserror");
    }

    #[test]
    fn valid_path_with_topic() {
        let p = MemoryPath::new("elasticsearch", Some("mapping"), "nested-dense-vector").unwrap();
        assert_eq!(p.entity, "elasticsearch");
        assert_eq!(p.topic, Some("mapping".to_string()));
        assert_eq!(p.name, "nested-dense-vector");
        assert_eq!(p.to_string(), "elasticsearch/mapping/nested-dense-vector");
    }

    #[test]
    fn fs_path_without_topic() {
        let p = MemoryPath::new("rust", None, "anyhow-vs-thiserror").unwrap();
        let fs = p.to_fs_path(Path::new("/home/user/.memory"));
        assert_eq!(
            fs,
            PathBuf::from("/home/user/.memory/entities/rust/anyhow-vs-thiserror.md")
        );
    }

    #[test]
    fn fs_path_with_topic() {
        let p = MemoryPath::new("elasticsearch", Some("mapping"), "nested-dense-vector").unwrap();
        let fs = p.to_fs_path(Path::new("/home/user/.memory"));
        assert_eq!(
            fs,
            PathBuf::from(
                "/home/user/.memory/entities/elasticsearch/mapping/nested-dense-vector.md"
            )
        );
    }

    #[test]
    fn reject_uppercase() {
        assert!(MemoryPath::new("Rust", None, "foo").is_err());
    }

    #[test]
    fn reject_empty() {
        assert!(MemoryPath::new("", None, "foo").is_err());
        assert!(MemoryPath::new("rust", None, "").is_err());
    }

    #[test]
    fn reject_special_chars() {
        assert!(MemoryPath::new("rust", None, "foo_bar").is_err());
        assert!(MemoryPath::new("rust", None, "foo bar").is_err());
        assert!(MemoryPath::new("rust", Some("a/b"), "foo").is_err());
    }

    #[test]
    fn validate_entity_name_ok() {
        assert!(validate_entity_name("elasticsearch").is_ok());
        assert!(validate_entity_name("my-project-1").is_ok());
    }

    #[test]
    fn validate_entity_name_err() {
        assert!(validate_entity_name("").is_err());
        assert!(validate_entity_name("MyProject").is_err());
    }
}
