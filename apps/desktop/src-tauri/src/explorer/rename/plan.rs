//! Turns steps into new names and decides which of them can be applied.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDateTime;
use serde::Serialize;

use super::steps::{apply_all, Meta, RenameStep};

const MAX_NAME_BYTES: usize = 255;
const ILLEGAL: [char; 8] = ['/', '\\', ':', '<', '>', '"', '|', '?'];
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

pub struct PlanItem {
    pub path: String,
    /// The folder that holds it. Names are only compared within a folder.
    pub parent: String,
    pub name: String,
    pub is_dir: bool,
    pub meta: Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Ok,
    Unchanged,
    Conflict,
    Invalid,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedItem {
    pub path: String,
    pub old_name: String,
    pub new_name: String,
    pub status: Status,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepError {
    pub index: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOutput {
    pub items: Vec<PlannedItem>,
    /// Steps that could not run (for example an invalid regex). While any exist nothing can be applied.
    pub step_errors: Vec<StepError>,
}

impl PlanOutput {
    /// Whether every item can be renamed or is left alone.
    pub fn can_apply(&self) -> bool {
        self.step_errors.is_empty()
            && self
                .items
                .iter()
                .all(|item| matches!(item.status, Status::Ok | Status::Unchanged))
    }
}

/// Why `name` cannot be a file name on any of the supported systems.
pub fn validate_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("The name is empty".into());
    }
    if name.len() > MAX_NAME_BYTES {
        return Err("The name is too long".into());
    }
    if name == "." || name == ".." {
        return Err("This name is not allowed".into());
    }
    if let Some(bad) = name
        .chars()
        .find(|c| c.is_control() || ILLEGAL.contains(c) || *c == '*')
    {
        return Err(if bad.is_control() {
            "The name has a control character".into()
        } else {
            format!("The name can't contain “{bad}”")
        });
    }
    if name.ends_with('.') || name.ends_with(' ') {
        return Err("The name can't end with a dot or a space".into());
    }
    let first = name.split('.').next().unwrap_or("").to_uppercase();
    if RESERVED.contains(&first.as_str()) {
        return Err("This name is reserved on Windows".into());
    }
    Ok(())
}

/// `taken(parent, name)` says whether `name` already exists in `parent`.
/// Names that belong to the batch itself are ignored, because they are being renamed away.
pub fn classify(
    items: &[PlanItem],
    new_names: &[String],
    taken: &dyn Fn(&str, &str) -> bool,
) -> Vec<PlannedItem> {
    let key = |parent: &str, name: &str| (parent.to_string(), name.to_lowercase());
    let validity: Vec<Result<(), String>> = new_names.iter().map(|name| validate_name(name)).collect();

    // The name every item will have once the batch is applied.
    let mut final_counts: HashMap<(String, String), usize> = HashMap::new();
    for ((item, new_name), validity) in items.iter().zip(new_names).zip(&validity) {
        let kept = if validity.is_ok() { new_name } else { &item.name };
        *final_counts.entry(key(&item.parent, kept)).or_default() += 1;
    }
    let batch_old_names: HashSet<(String, String)> = items
        .iter()
        .map(|item| key(&item.parent, &item.name))
        .collect();

    items
        .iter()
        .zip(new_names)
        .zip(validity)
        .map(|((item, new_name), validity)| {
            let planned = |status, reason: Option<String>, name: &String| PlannedItem {
                path: item.path.clone(),
                old_name: item.name.clone(),
                new_name: name.clone(),
                status,
                reason,
            };
            if *new_name == item.name {
                return planned(Status::Unchanged, None, new_name);
            }
            if let Err(reason) = validity {
                return planned(Status::Invalid, Some(reason), new_name);
            }
            let lowered = key(&item.parent, new_name);
            if final_counts.get(&lowered).copied().unwrap_or(0) > 1 {
                return planned(
                    Status::Conflict,
                    Some("Another item would get the same name".into()),
                    new_name,
                );
            }
            if taken(&item.parent, new_name) && !batch_old_names.contains(&lowered) {
                return planned(
                    Status::Conflict,
                    Some("An item with this name already exists".into()),
                    new_name,
                );
            }
            planned(Status::Ok, None, new_name)
        })
        .collect()
}

/// Computes the new names with `steps` and classifies them.
pub fn plan(
    items: &[PlanItem],
    steps: &[RenameStep],
    today: NaiveDateTime,
    taken: &dyn Fn(&str, &str) -> bool,
) -> PlanOutput {
    let mut step_errors: Vec<StepError> = Vec::new();
    let new_names: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            match apply_all(&item.name, item.is_dir, steps, index, &item.meta, today) {
                Ok(name) => name,
                Err((step, message)) => {
                    if !step_errors.iter().any(|error| error.index == step) {
                        step_errors.push(StepError {
                            index: step,
                            message,
                        });
                    }
                    item.name.clone()
                }
            }
        })
        .collect();
    PlanOutput {
        items: classify(items, &new_names, taken),
        step_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explorer::rename::steps::{StepKind, Target};
    use chrono::NaiveDate;

    fn today() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 24)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    fn item(name: &str) -> PlanItem {
        item_in("/photos", name)
    }

    fn item_in(parent: &str, name: &str) -> PlanItem {
        PlanItem {
            path: format!("{parent}/{name}"),
            parent: parent.into(),
            name: name.into(),
            is_dir: false,
            meta: Meta::default(),
        }
    }

    fn nothing_exists(_: &str, _: &str) -> bool {
        false
    }

    fn statuses(planned: &[PlannedItem]) -> Vec<Status> {
        planned.iter().map(|item| item.status).collect()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn valid_names_pass_and_bad_ones_say_why() {
        assert!(validate_name("photo 1.jpg").is_ok());
        assert!(validate_name(".gitignore").is_ok());
        assert_eq!(validate_name("").unwrap_err(), "The name is empty");
        assert_eq!(validate_name("   ").unwrap_err(), "The name is empty");
        assert_eq!(validate_name("a/b").unwrap_err(), "The name can't contain “/”");
        assert_eq!(validate_name("a:b").unwrap_err(), "The name can't contain “:”");
        assert!(validate_name("what?.txt").is_err());
        assert!(validate_name("star*.txt").is_err());
        assert!(validate_name("tab\tname").is_err());
        assert_eq!(validate_name("name.").unwrap_err(), "The name can't end with a dot or a space");
        assert_eq!(validate_name("name ").unwrap_err(), "The name can't end with a dot or a space");
        assert!(validate_name("..").is_err());
        assert_eq!(validate_name("con.txt").unwrap_err(), "This name is reserved on Windows");
        assert!(validate_name("COM1").is_err());
        assert!(validate_name("console.txt").is_ok());
        assert_eq!(validate_name(&"a".repeat(256)).unwrap_err(), "The name is too long");
        assert!(validate_name(&"a".repeat(255)).is_ok());
    }

    #[test]
    fn unchanged_invalid_and_ok_statuses() {
        let items = [item("a.txt"), item("b.txt"), item("c.txt")];
        let planned = classify(&items, &names(&["a.txt", "b?.txt", "z.txt"]), &nothing_exists);
        assert_eq!(statuses(&planned), [Status::Unchanged, Status::Invalid, Status::Ok]);
        assert_eq!(planned[1].reason.as_deref(), Some("The name can't contain “?”"));
    }

    #[test]
    fn two_items_getting_the_same_name_both_conflict() {
        let items = [item("a.txt"), item("b.txt")];
        let planned = classify(&items, &names(&["same.txt", "SAME.txt"]), &nothing_exists);
        assert_eq!(statuses(&planned), [Status::Conflict, Status::Conflict]);
    }

    #[test]
    fn taking_the_name_of_an_unchanged_item_conflicts() {
        let items = [item("a.txt"), item("b.txt")];
        let planned = classify(&items, &names(&["a.txt", "a.txt"]), &nothing_exists);
        // The first keeps its name; the second would collide with it.
        assert_eq!(statuses(&planned), [Status::Unchanged, Status::Conflict]);
    }

    #[test]
    fn an_existing_outside_item_conflicts() {
        let items = [item("a.txt")];
        let exists = |parent: &str, name: &str| parent == "/photos" && name == "taken.txt";
        let planned = classify(&items, &names(&["taken.txt"]), &exists);
        assert_eq!(planned[0].status, Status::Conflict);
        assert_eq!(planned[0].reason.as_deref(), Some("An item with this name already exists"));
    }

    #[test]
    fn swaps_and_chains_inside_the_batch_are_fine() {
        let items = [item("a.txt"), item("b.txt")];
        let exists = |_: &str, name: &str| name == "a.txt" || name == "b.txt";
        let swap = classify(&items, &names(&["b.txt", "a.txt"]), &exists);
        assert_eq!(statuses(&swap), [Status::Ok, Status::Ok]);
        let items = [item("a"), item("b")];
        let exists = |_: &str, name: &str| name == "a" || name == "b";
        let chain = classify(&items, &names(&["b", "c"]), &exists);
        assert_eq!(statuses(&chain), [Status::Ok, Status::Ok]);
    }

    #[test]
    fn a_change_of_case_only_is_not_a_conflict() {
        let items = [item("photo.JPG")];
        let exists = |_: &str, name: &str| name.eq_ignore_ascii_case("photo.JPG");
        let planned = classify(&items, &names(&["photo.jpg"]), &exists);
        assert_eq!(planned[0].status, Status::Ok);
    }

    #[test]
    fn the_same_name_in_different_folders_is_fine() {
        let items = [item_in("/one", "a.txt"), item_in("/two", "b.txt")];
        let planned = classify(&items, &names(&["x.txt", "x.txt"]), &nothing_exists);
        assert_eq!(statuses(&planned), [Status::Ok, Status::Ok]);
    }

    #[test]
    fn plan_applies_the_steps_in_list_order() {
        let items = [item("IMG_1.JPG"), item("IMG_2.JPG")];
        let steps = [
            RenameStep {
                enabled: true,
                kind: StepKind::Replace {
                    find: "IMG_".into(),
                    with: "Trip ".into(),
                    regex: false,
                    case_sensitive: true,
                    target: Target::Name,
                },
            },
            RenameStep {
                enabled: true,
                kind: StepKind::Replace {
                    find: "JPG".into(),
                    with: "jpg".into(),
                    regex: false,
                    case_sensitive: true,
                    target: Target::Extension,
                },
            },
        ];
        let output = plan(&items, &steps, today(), &nothing_exists);
        let new: Vec<_> = output.items.iter().map(|item| item.new_name.as_str()).collect();
        assert_eq!(new, ["Trip 1.jpg", "Trip 2.jpg"]);
        assert!(output.can_apply());
    }

    #[test]
    fn plan_reports_a_broken_step_once_and_blocks_applying() {
        let items = [item("a.txt"), item("b.txt")];
        let steps = [RenameStep {
            enabled: true,
            kind: StepKind::Replace {
                find: "(".into(),
                with: "x".into(),
                regex: true,
                case_sensitive: true,
                target: Target::Name,
            },
        }];
        let output = plan(&items, &steps, today(), &nothing_exists);
        assert_eq!(output.step_errors.len(), 1);
        assert_eq!(output.step_errors[0].index, 0);
        assert!(!output.can_apply());
        assert_eq!(statuses(&output.items), [Status::Unchanged, Status::Unchanged]);
    }

    #[test]
    fn can_apply_is_false_with_any_conflict() {
        let items = [item("a.txt"), item("b.txt")];
        let steps = [RenameStep {
            enabled: true,
            kind: StepKind::Replace {
                find: "[ab]".into(),
                with: "z".into(),
                regex: true,
                case_sensitive: true,
                target: Target::Name,
            },
        }];
        let output = plan(&items, &steps, today(), &nothing_exists);
        assert_eq!(statuses(&output.items), [Status::Conflict, Status::Conflict]);
        assert!(!output.can_apply());
    }

    #[test]
    fn no_steps_means_nothing_changes() {
        let items = [item("a.txt")];
        let output = plan(&items, &[], today(), &nothing_exists);
        assert_eq!(statuses(&output.items), [Status::Unchanged]);
        assert!(output.can_apply());
    }
}
