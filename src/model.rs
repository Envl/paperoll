use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceData {
    pub active_roll_id: Uuid,
    pub rolls: Vec<RollData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollData {
    pub id: Uuid,
    pub title: String,
    pub snippets: Vec<SnippetData>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnippetData {
    pub id: Uuid,
    pub text: String,
    pub language: String,
}

impl WorkspaceData {
    pub fn empty() -> Self {
        let roll = RollData::empty(1);
        Self {
            active_roll_id: roll.id,
            rolls: vec![roll],
        }
    }

    pub fn normalize(mut self) -> Self {
        if self.rolls.is_empty() {
            return Self::empty();
        }

        for (ix, roll) in self.rolls.iter_mut().enumerate() {
            if roll.snippets.is_empty() {
                roll.snippets.push(SnippetData::empty());
            }
            if roll.title.trim().is_empty() {
                roll.title = format!("Roll {}", ix + 1);
            }
        }

        if !self.rolls.iter().any(|roll| roll.id == self.active_roll_id) {
            self.active_roll_id = self.rolls[0].id;
        }
        self
    }
}

impl RollData {
    pub fn empty(number: usize) -> Self {
        Self {
            id: Uuid::new_v4(),
            title: format!("Roll {number}"),
            snippets: vec![SnippetData::empty()],
        }
    }
}

impl SnippetData {
    pub fn empty() -> Self {
        Self {
            id: Uuid::new_v4(),
            text: String::new(),
            language: "auto".to_string(),
        }
    }
}

pub fn should_delete_snippet_on_backspace(text: &str, snippet_count: usize) -> bool {
    text.is_empty() && snippet_count > 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backspace_keeps_a_single_empty_page() {
        assert!(!should_delete_snippet_on_backspace("", 1));
    }

    #[test]
    fn backspace_deletes_only_an_already_empty_extra_page() {
        assert!(should_delete_snippet_on_backspace("", 2));
        assert!(!should_delete_snippet_on_backspace("still here", 2));
    }

    #[test]
    fn normalization_repairs_invalid_active_roll_and_empty_pages() {
        let mut workspace = WorkspaceData::empty();
        workspace.active_roll_id = Uuid::new_v4();
        workspace.rolls[0].title.clear();
        workspace.rolls[0].snippets.clear();

        let normalized = workspace.normalize();
        assert_eq!(normalized.active_roll_id, normalized.rolls[0].id);
        assert_eq!(normalized.rolls[0].title, "Roll 1");
        assert_eq!(normalized.rolls[0].snippets.len(), 1);
    }
}
