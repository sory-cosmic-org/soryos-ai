//! Sidebar view-model: conversation list rows, grouped like ChatGPT.

use crate::state::ConversationSummary;
use assistant_core::Conversation;
use chrono::Local;

/// Build sidebar rows, most recently updated first.
pub fn summarize(conversations: &[Conversation]) -> Vec<ConversationSummary> {
    let mut sorted: Vec<&Conversation> = conversations.iter().collect();
    sorted.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
    sorted
        .into_iter()
        .map(|c| ConversationSummary {
            id: c.id,
            title: if c.title.is_empty() {
                "(sans titre)".to_string()
            } else {
                c.title.clone()
            },
            updated: c.updated_at.format("%d/%m %H:%M").to_string(),
            updated_days_ago: days_ago(c.updated_at),
        })
        .collect()
}

fn days_ago(when: chrono::DateTime<chrono::Utc>) -> i64 {
    let today = Local::now().date_naive();
    let then = when.with_timezone(&Local).date_naive();
    (today - then).num_days()
}

/// One ChatGPT-style history section (`Aujourd'hui`, `Hier`, ...).
#[derive(Debug, Clone)]
pub struct HistoryGroup {
    pub label: String,
    pub items: Vec<ConversationSummary>,
}

/// Group rows into date sections, optionally filtered by a search query.
/// Empty groups are omitted; order is newest section first.
pub fn group_history(rows: &[ConversationSummary], filter: &str) -> Vec<HistoryGroup> {
    let query = filter.trim().to_lowercase();
    let mut today = vec![];
    let mut yesterday = vec![];
    let mut last_week = vec![];
    let mut older = vec![];
    for row in rows {
        if !query.is_empty() && !row.title.to_lowercase().contains(&query) {
            continue;
        }
        match row.updated_days_ago {
            0 => today.push(row.clone()),
            1 => yesterday.push(row.clone()),
            2..=7 => last_week.push(row.clone()),
            _ => older.push(row.clone()),
        }
    }
    let mut groups = vec![];
    for (label, items) in [
        ("Aujourd'hui", today),
        ("Hier", yesterday),
        ("7 derniers jours", last_week),
        ("Plus ancien", older),
    ] {
        if !items.is_empty() {
            groups.push(HistoryGroup {
                label: label.to_string(),
                items,
            });
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use assistant_core::{ChatMessage, Conversation};

    fn conversation_days_ago(title: &str, days: i64) -> Conversation {
        let mut c = Conversation::new(title);
        c.push(ChatMessage::user("hello"));
        c.updated_at = chrono::Utc::now() - chrono::Duration::days(days);
        c
    }

    #[test]
    fn groups_by_recency_and_filters() {
        let convs = vec![
            conversation_days_ago("aujourd'hui", 0),
            conversation_days_ago("hier", 1),
            conversation_days_ago("vieux sujet", 30),
        ];
        let rows = summarize(&convs);
        let groups = group_history(&rows, "");
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0].label, "Aujourd'hui");
        let filtered = group_history(&rows, "vieux");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].items[0].title, "vieux sujet");
    }
}
