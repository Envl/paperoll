use std::{
    cmp::Ordering,
    fs, io,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use uuid::Uuid;

use crate::{
    detection::LanguageSelection,
    model::{RollData, SnippetData, WorkspaceData},
};

pub struct WorkspaceStore {
    root: PathBuf,
}

impl WorkspaceStore {
    pub fn application_default() -> Self {
        let root = std::env::var_os("PAPEROLL_WORKSPACE_PATH")
            .map(PathBuf::from)
            .or_else(|| {
                ProjectDirs::from("com", "Lumik", "Paperoll")
                    .map(|dirs| dirs.data_local_dir().join("rolls"))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("paperoll-rolls"));
        Self { root }
    }

    #[cfg(test)]
    fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn load(&self) -> io::Result<WorkspaceData> {
        self.recover_interrupted_save()?;
        if !self.root.exists() {
            return Ok(WorkspaceData::empty());
        }
        let roll_entries = visible_children(&self.root, ChildKind::Directory)?;
        let mut rolls = Vec::with_capacity(roll_entries.len());

        for (roll_ix, entry) in roll_entries.into_iter().enumerate() {
            let snippet_entries = visible_children(&entry.path, ChildKind::File)?;
            let mut snippets = Vec::with_capacity(snippet_entries.len().max(1));

            for snippet in snippet_entries {
                let language = LanguageSelection::from_file_extension(
                    snippet.path.extension().and_then(|value| value.to_str()),
                );
                snippets.push(SnippetData {
                    id: Uuid::new_v4(),
                    text: fs::read_to_string(snippet.path)?,
                    language: language.persisted().to_string(),
                });
            }

            if snippets.is_empty() {
                snippets.push(SnippetData::empty());
            }
            rolls.push(RollData {
                id: Uuid::new_v4(),
                title: roll_title(&entry.name, roll_ix + 1),
                snippets,
            });
        }

        if rolls.is_empty() {
            return Ok(WorkspaceData::empty());
        }
        Ok(WorkspaceData {
            active_roll_id: rolls[0].id,
            rolls,
        }
        .normalize())
    }

    pub fn initialize_if_empty(&self, workspace: &WorkspaceData) -> io::Result<()> {
        self.recover_interrupted_save()?;
        if self.root.exists() {
            if !self.root.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "workspace path must be a directory",
                ));
            }
            if !visible_children(&self.root, ChildKind::Directory)?.is_empty() {
                return Ok(());
            }
        }
        self.save(workspace)
    }

    pub fn save_snippet(
        &self,
        roll_ix: usize,
        roll_title: &str,
        snippet_ix: usize,
        language: LanguageSelection,
        text: &str,
    ) -> io::Result<()> {
        self.recover_interrupted_save()?;
        ensure_directory(&self.root)?;
        let roll_path = visible_children(&self.root, ChildKind::Directory)?
            .get(roll_ix)
            .map(|roll| roll.path.clone())
            .unwrap_or_else(|| self.root.join(roll_folder_name(roll_ix, roll_title)));
        ensure_directory(&roll_path)?;

        let existing_path = visible_children(&roll_path, ChildKind::File)?
            .get(snippet_ix)
            .map(|snippet| snippet.path.clone());
        let path = existing_path
            .clone()
            .unwrap_or_else(|| roll_path.join(snippet_file_name(snippet_ix, language)));
        let temporary = roll_path.join(format!(".paperoll-{}.tmp", Uuid::new_v4()));
        fs::write(&temporary, text)?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(temporary);
            return Err(error);
        }

        if existing_path.is_none() {
            let expected_order = snippet_ix + 1;
            for sibling in visible_children(&roll_path, ChildKind::File)? {
                if sibling.order == Some(expected_order) && sibling.path != path {
                    fs::remove_file(sibling.path)?;
                }
            }
        }
        Ok(())
    }

    pub fn save(&self, workspace: &WorkspaceData) -> io::Result<()> {
        let parent = self.root.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "workspace directory has no parent",
            )
        })?;
        fs::create_dir_all(parent)?;
        self.recover_interrupted_save()?;
        if self.root.exists() && !self.root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "workspace path must be a directory",
            ));
        }

        let staging = self.sidecar("next")?;
        let backup = self.sidecar("previous")?;
        remove_path_if_exists(&staging)?;
        ensure_directory(&staging)?;
        if let Err(error) = write_workspace_tree(&staging, workspace) {
            let _ = remove_path_if_exists(&staging);
            return Err(error);
        }

        remove_path_if_exists(&backup)?;
        let had_workspace = self.root.exists();
        if had_workspace {
            fs::rename(&self.root, &backup)?;
        }
        if let Err(error) = fs::rename(&staging, &self.root) {
            if had_workspace {
                let _ = fs::rename(&backup, &self.root);
            }
            let _ = remove_path_if_exists(&staging);
            return Err(error);
        }
        let _ = remove_path_if_exists(&backup);
        Ok(())
    }

    fn recover_interrupted_save(&self) -> io::Result<()> {
        let Some(parent) = self.root.parent() else {
            return Ok(());
        };
        if !parent.exists() {
            return Ok(());
        }

        let staging = self.sidecar("next")?;
        let backup = self.sidecar("previous")?;
        if self.root.exists() {
            if !self.root.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "workspace path must be a directory",
                ));
            }
            remove_path_if_exists(&staging)?;
            remove_path_if_exists(&backup)?;
        } else if backup.exists() {
            fs::rename(&backup, &self.root)?;
            remove_path_if_exists(&staging)?;
        } else if staging.exists() {
            fs::rename(&staging, &self.root)?;
        }
        Ok(())
    }

    fn sidecar(&self, suffix: &str) -> io::Result<PathBuf> {
        let parent = self.root.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "workspace directory has no parent",
            )
        })?;
        let name = self
            .root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "workspace directory must have a UTF-8 name",
                )
            })?;
        Ok(parent.join(format!("{name}.{suffix}")))
    }
}

#[derive(Clone, Copy)]
enum ChildKind {
    Directory,
    File,
}

struct Child {
    path: PathBuf,
    name: String,
    order: Option<usize>,
}

fn visible_children(parent: &Path, kind: ChildKind) -> io::Result<Vec<Child>> {
    let mut children = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let matches_kind = match kind {
            ChildKind::Directory => file_type.is_dir(),
            ChildKind::File => file_type.is_file(),
        };
        if !matches_kind {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        children.push(Child {
            order: ordered_prefix(&name),
            path: entry.path(),
            name,
        });
    }

    children.sort_by(|left, right| match (left.order, right.order) {
        (Some(left_order), Some(right_order)) => left_order
            .cmp(&right_order)
            .then_with(|| left.name.cmp(&right.name)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.name.cmp(&right.name),
    });
    Ok(children)
}

fn ordered_prefix(name: &str) -> Option<usize> {
    let digit_count = name.bytes().take_while(u8::is_ascii_digit).count();
    if digit_count == 0 {
        return None;
    }
    let separator = name.as_bytes().get(digit_count).copied();
    if !matches!(separator, None | Some(b' ' | b'.')) {
        return None;
    }
    name[..digit_count].parse().ok()
}

fn roll_title(name: &str, fallback_number: usize) -> String {
    let digit_count = name.bytes().take_while(u8::is_ascii_digit).count();
    let title = if digit_count > 0 && name.as_bytes().get(digit_count) == Some(&b' ') {
        name[digit_count + 1..].trim()
    } else {
        name.trim()
    };
    if title.is_empty() {
        format!("Roll {fallback_number}")
    } else {
        title.to_string()
    }
}

fn write_workspace_tree(root: &Path, workspace: &WorkspaceData) -> io::Result<()> {
    for (roll_ix, roll) in workspace.rolls.iter().enumerate() {
        let roll_path = root.join(roll_folder_name(roll_ix, &roll.title));
        fs::create_dir(&roll_path)?;
        for (snippet_ix, snippet) in roll.snippets.iter().enumerate() {
            let language = LanguageSelection::from_persisted(&snippet.language);
            fs::write(
                roll_path.join(snippet_file_name(snippet_ix, language)),
                &snippet.text,
            )?;
        }
    }
    Ok(())
}

fn roll_folder_name(index: usize, title: &str) -> String {
    format!("{:03} {}", index + 1, safe_component(title, "Untitled"))
}

fn snippet_file_name(index: usize, language: LanguageSelection) -> String {
    let number = format!("{:03}", index + 1);
    match language.file_extension() {
        Some(extension) => format!("{number}.{extension}"),
        None => number,
    }
}

fn safe_component(value: &str, fallback: &str) -> String {
    let mut result: String = value
        .trim()
        .chars()
        .filter_map(|character| match character {
            '/' | ':' => Some('-'),
            character if character.is_control() => None,
            character => Some(character),
        })
        .collect();
    while result.ends_with('.') || result.ends_with(' ') {
        result.pop();
    }
    if result.is_empty() {
        fallback.to_string()
    } else {
        result
    }
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    if path.exists() && !path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} must be a directory", path.display()),
        ));
    }
    fs::create_dir_all(path)
}

fn remove_path_if_exists(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if path.exists() {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_directory() -> PathBuf {
        std::env::temp_dir().join(format!("paperoll-test-{}", Uuid::new_v4()))
    }

    #[test]
    fn saves_rolls_as_folders_and_snippets_as_files() {
        let directory = test_directory();
        let store = WorkspaceStore::at(directory.join("rolls"));
        let mut workspace = WorkspaceData::empty();
        workspace.rolls[0].title = "Ideas / Drafts".to_string();
        workspace.rolls[0].snippets[0].text = "auto text".to_string();
        workspace.rolls[0].snippets.push(SnippetData {
            id: Uuid::new_v4(),
            text: "fn main() {}".to_string(),
            language: "rust".to_string(),
        });

        store.save(&workspace).unwrap();

        let roll = directory.join("rolls/001 Ideas - Drafts");
        assert_eq!(fs::read_to_string(roll.join("001")).unwrap(), "auto text");
        assert_eq!(
            fs::read_to_string(roll.join("002.rs")).unwrap(),
            "fn main() {}"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn saving_a_renamed_roll_replaces_its_folder() {
        let directory = test_directory();
        let root = directory.join("rolls");
        let store = WorkspaceStore::at(root.clone());
        let mut workspace = WorkspaceData::empty();
        workspace.rolls[0].snippets[0].text = "kept".to_string();
        store.save(&workspace).unwrap();

        workspace.rolls[0].title = "Reference".to_string();
        store.save(&workspace).unwrap();

        assert!(!root.join("001 Roll 1").exists());
        assert_eq!(
            fs::read_to_string(root.join("001 Reference/001")).unwrap(),
            "kept"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_workspace_loads_as_an_empty_roll_without_writing() {
        let directory = test_directory();
        let root = directory.join("rolls");

        let loaded = WorkspaceStore::at(root.clone()).load().unwrap();

        assert_eq!(loaded.rolls.len(), 1);
        assert_eq!(loaded.rolls[0].snippets.len(), 1);
        assert!(!root.exists());
    }

    #[test]
    fn loads_ordered_and_external_folders_and_files() {
        let directory = test_directory();
        let root = directory.join("rolls");
        fs::create_dir_all(root.join("002 Work")).unwrap();
        fs::create_dir_all(root.join("001 Notes")).unwrap();
        fs::create_dir_all(root.join("Scratch")).unwrap();
        fs::write(root.join("001 Notes/002.rs"), "fn main() {}").unwrap();
        fs::write(root.join("001 Notes/001"), "hello").unwrap();
        fs::write(root.join("002 Work/todo.md"), "- one\n- two").unwrap();
        fs::write(root.join("Scratch/note.txt"), "plain").unwrap();

        let loaded = WorkspaceStore::at(root).load().unwrap();

        assert_eq!(
            loaded
                .rolls
                .iter()
                .map(|roll| roll.title.as_str())
                .collect::<Vec<_>>(),
            ["Notes", "Work", "Scratch"]
        );
        assert_eq!(loaded.rolls[0].snippets[0].text, "hello");
        assert_eq!(loaded.rolls[0].snippets[0].language, "auto");
        assert_eq!(loaded.rolls[0].snippets[1].language, "rust");
        assert_eq!(loaded.rolls[1].snippets[0].language, "markdown");
        assert_eq!(loaded.rolls[2].snippets[0].language, "text");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn targeted_snippet_save_does_not_rebuild_siblings() {
        let directory = test_directory();
        let root = directory.join("rolls");
        let store = WorkspaceStore::at(root.clone());
        let mut workspace = WorkspaceData::empty();
        workspace.rolls[0].snippets.push(SnippetData::empty());
        workspace.rolls[0].snippets[1].text = "untouched".to_string();
        store.save(&workspace).unwrap();

        store
            .save_snippet(0, "Roll 1", 0, LanguageSelection::Auto, "updated")
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("001 Roll 1/001")).unwrap(),
            "updated"
        );
        assert_eq!(
            fs::read_to_string(root.join("001 Roll 1/002")).unwrap(),
            "untouched"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn targeted_snippet_save_preserves_external_names() {
        let directory = test_directory();
        let root = directory.join("rolls");
        fs::create_dir_all(root.join("Scratch")).unwrap();
        fs::write(root.join("Scratch/010"), "first").unwrap();
        fs::write(root.join("Scratch/note.md"), "old").unwrap();
        let store = WorkspaceStore::at(root.clone());

        store
            .save_snippet(
                0,
                "Scratch",
                1,
                LanguageSelection::Explicit(crate::detection::DetectedLanguage::Markdown),
                "updated",
            )
            .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("Scratch/note.md")).unwrap(),
            "updated"
        );
        assert!(!root.join("001 Scratch").exists());
        assert_eq!(
            fs::read_to_string(root.join("Scratch/010")).unwrap(),
            "first"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn recovers_previous_tree_after_interrupted_swap() {
        let directory = test_directory();
        let root = directory.join("rolls");
        let backup = directory.join("rolls.previous");
        fs::create_dir_all(backup.join("001 Recovered")).unwrap();
        fs::write(backup.join("001 Recovered/001"), "safe").unwrap();

        let loaded = WorkspaceStore::at(root).load().unwrap();

        assert_eq!(loaded.rolls[0].title, "Recovered");
        assert_eq!(loaded.rolls[0].snippets[0].text, "safe");
        fs::remove_dir_all(directory).unwrap();
    }
}
