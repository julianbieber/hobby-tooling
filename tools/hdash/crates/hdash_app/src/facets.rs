//! The chip filters of the item list: which chips an item matches, whether it passes the chips
//! that are on, and how many items each chip would leave. Chips within one group are
//! alternatives; groups combine with AND.

use std::collections::BTreeSet;
use std::time::{Duration, SystemTime};

use crate::model::item::{Item, Stage};
use crate::session::SessionState;

/// The State group's chips: where a task is in its lifecycle, and its block.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum StateChip {
    Ready,
    Blocked,
    NotBlocked,
    NotStarted,
    Removed,
    Orphan,
}

/// The Active group's chips, by the notes' last activity: under a day, under a week, older. A task
/// active today also matches `Week`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum ActiveChip {
    Today,
    Week,
    Older,
}

/// The Session group's chips: the task's Claude session state, or no live session.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SessionChip {
    Busy,
    Idle,
    NoSession,
}

/// The chip groups, in the order the chip bar shows them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Group {
    State,
    Pr,
    Repos,
    Phase,
    Active,
    Session,
}

impl Group {
    pub const ALL: [Group; 6] = [
        Group::State,
        Group::Pr,
        Group::Repos,
        Group::Phase,
        Group::Active,
        Group::Session,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Group::State => "State",
            Group::Pr => "PR",
            Group::Repos => "Repos",
            Group::Phase => "Phase",
            Group::Active => "Active",
            Group::Session => "Session",
        }
    }
}

/// One filter chip. `Phase(None)` matches items with no `build-task.json` phase.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Chip {
    State(StateChip),
    AwaitingReview,
    Dirty,
    Phase(Option<String>),
    Active(ActiveChip),
    Session(SessionChip),
}

impl Chip {
    pub fn group(&self) -> Group {
        match self {
            Chip::State(_) => Group::State,
            Chip::AwaitingReview => Group::Pr,
            Chip::Dirty => Group::Repos,
            Chip::Phase(_) => Group::Phase,
            Chip::Active(_) => Group::Active,
            Chip::Session(_) => Group::Session,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Chip::State(StateChip::Ready) => "ready".into(),
            Chip::State(StateChip::Blocked) => "blocked".into(),
            Chip::State(StateChip::NotBlocked) => "not blocked".into(),
            Chip::State(StateChip::NotStarted) => "not started".into(),
            Chip::State(StateChip::Removed) => "removed".into(),
            Chip::State(StateChip::Orphan) => "orphan".into(),
            Chip::AwaitingReview => "awaiting review".into(),
            Chip::Dirty => "dirty".into(),
            Chip::Phase(Some(p)) => p.clone(),
            Chip::Phase(None) => "no phase".into(),
            Chip::Active(ActiveChip::Today) => "today".into(),
            Chip::Active(ActiveChip::Week) => "7 days".into(),
            Chip::Active(ActiveChip::Older) => "older".into(),
            Chip::Session(SessionChip::Busy) => "busy".into(),
            Chip::Session(SessionChip::Idle) => "idle".into(),
            Chip::Session(SessionChip::NoSession) => "no session".into(),
        }
    }

    fn fixed(group: Group) -> Vec<Chip> {
        match group {
            Group::State => [
                StateChip::Ready,
                StateChip::Blocked,
                StateChip::NotBlocked,
                StateChip::NotStarted,
                StateChip::Removed,
                StateChip::Orphan,
            ]
            .into_iter()
            .map(Chip::State)
            .collect(),
            Group::Pr => vec![Chip::AwaitingReview],
            Group::Repos => vec![Chip::Dirty],
            Group::Phase => Vec::new(),
            Group::Active => [ActiveChip::Today, ActiveChip::Week, ActiveChip::Older]
                .into_iter()
                .map(Chip::Active)
                .collect(),
            Group::Session => [SessionChip::Busy, SessionChip::Idle, SessionChip::NoSession]
                .into_iter()
                .map(Chip::Session)
                .collect(),
        }
    }
}

/// The chips that are on. Empty means no chip filtering.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Filters(pub BTreeSet<Chip>);

impl Filters {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn is_on(&self, chip: &Chip) -> bool {
        self.0.contains(chip)
    }

    pub fn toggle(&mut self, chip: Chip) {
        if !self.0.remove(&chip) {
            self.0.insert(chip);
        }
    }

    /// The groups with at least one chip on, in display order.
    pub fn active_groups(&self) -> Vec<Group> {
        let groups: BTreeSet<Group> = self.0.iter().map(Chip::group).collect();
        groups.into_iter().collect()
    }
}

/// Every chip one item matches.
#[derive(Clone, Debug, Default)]
pub struct Facts(BTreeSet<Chip>);

impl Facts {
    pub fn matches(&self, chip: &Chip) -> bool {
        self.0.contains(chip)
    }

    fn phases(&self) -> impl Iterator<Item = &Chip> {
        self.0.iter().filter(|c| c.group() == Group::Phase)
    }
}

/// The chips `item` matches, given its session state and the current time.
pub fn facts(item: &Item<'_>, session: Option<SessionState>, now: SystemTime) -> Facts {
    let task = item.task;
    let stage = item.stage();
    let mut chips = BTreeSet::new();
    if stage != Stage::Done {
        if task.ready() {
            chips.insert(Chip::State(StateChip::Ready));
        }
        if task.blocked() && !task.ready() {
            chips.insert(Chip::State(StateChip::Blocked));
        } else {
            chips.insert(Chip::State(StateChip::NotBlocked));
        }
    }
    match stage {
        Stage::NotStarted => {
            chips.insert(Chip::State(StateChip::NotStarted));
        }
        Stage::Removed => {
            chips.insert(Chip::State(StateChip::Removed));
        }
        Stage::Orphan => {
            chips.insert(Chip::State(StateChip::Orphan));
        }
        _ => {}
    }
    if task.awaiting_review() > 0 {
        chips.insert(Chip::AwaitingReview);
    }
    if task.present && task.dirty_total() > 0 {
        chips.insert(Chip::Dirty);
    }
    chips.insert(Chip::Phase(task.phase.clone()));
    let age = task
        .last_active
        .map(|t| now.duration_since(t).unwrap_or(Duration::ZERO));
    match age {
        Some(a) if a < Duration::from_secs(86_400) => {
            chips.insert(Chip::Active(ActiveChip::Today));
            chips.insert(Chip::Active(ActiveChip::Week));
        }
        Some(a) if a < Duration::from_secs(7 * 86_400) => {
            chips.insert(Chip::Active(ActiveChip::Week));
        }
        _ => {
            chips.insert(Chip::Active(ActiveChip::Older));
        }
    }
    chips.insert(Chip::Session(match session {
        Some(SessionState::Busy) => SessionChip::Busy,
        Some(SessionState::Idle) => SessionChip::Idle,
        None => SessionChip::NoSession,
    }));
    Facts(chips)
}

/// The chips a group offers, in display order: its fixed chips, or for Phase every phase some item
/// carries plus every Phase chip that is on (so a chip whose value left the data can still be
/// turned off), with "no phase" last.
pub fn candidates(group: Group, items: &[Facts], filters: &Filters) -> Vec<Chip> {
    if group != Group::Phase {
        return Chip::fixed(group);
    }
    let mut values: BTreeSet<Chip> = items.iter().flat_map(|f| f.phases().cloned()).collect();
    values.extend(filters.0.iter().filter(|c| c.group() == group).cloned());
    let (none, some): (Vec<Chip>, Vec<Chip>) = values
        .into_iter()
        .partition(|c| matches!(c, Chip::Phase(None)));
    some.into_iter().chain(none).collect()
}

fn passes_except(facts: &Facts, filters: &Filters, skip: Option<Group>) -> bool {
    filters
        .active_groups()
        .into_iter()
        .filter(|g| Some(*g) != skip)
        .all(|g| {
            filters
                .0
                .iter()
                .filter(|c| c.group() == g)
                .any(|c| facts.matches(c))
        })
}

/// Whether an item passes the chips that are on: in every group with a chip on, it matches one of
/// that group's chips.
pub fn passes(facts: &Facts, filters: &Filters) -> bool {
    passes_except(facts, filters, None)
}

/// How many of `items` the list would show with `chip` on: every other group applied as it is, the
/// chip's own group replaced by this one chip.
pub fn count<'a>(
    items: impl IntoIterator<Item = &'a Facts>,
    filters: &Filters,
    chip: &Chip,
) -> usize {
    items
        .into_iter()
        .filter(|f| f.matches(chip) && passes_except(f, filters, Some(chip.group())))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::task::{Block, GitInfo, Repo, Task, TaskId};
    use std::path::PathBuf;

    const DAY: u64 = 86_400;

    fn task(name: &str) -> Task {
        let id = TaskId {
            project: "p".into(),
            topic: "t".into(),
            name: name.into(),
        };
        let dir = PathBuf::from("/nowhere").join(id.rel());
        Task {
            repos: vec![Repo {
                path: dir.clone(),
                git: GitInfo::default(),
            }],
            id,
            dir,
            adopted: true,
            present: true,
            done: false,
            block: None,
            backlog: None,
            title: None,
            has_branch: true,
            prs: Vec::new(),
            aspects: Vec::new(),
            last_active: None,
            coordinator: None,
            phase: None,
            todo: Vec::new(),
            waiting_on: None,
            session_name: String::new(),
        }
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(100 * DAY)
    }

    fn of(task: &Task, session: Option<SessionState>) -> Facts {
        facts(&Item { task }, session, now())
    }

    fn on(chips: &[Chip]) -> Filters {
        Filters(chips.iter().cloned().collect())
    }

    // Pins the State group: lifecycle stages and blocks each carry their own chip.
    #[test]
    fn state_chips_follow_stage_and_block() {
        let mut orphan = task("o");
        orphan.adopted = false;
        assert!(of(&orphan, None).matches(&Chip::State(StateChip::Orphan)));
        let mut unstarted = task("u");
        unstarted.present = false;
        unstarted.has_branch = false;
        assert!(of(&unstarted, None).matches(&Chip::State(StateChip::NotStarted)));
        let mut lifted = task("l");
        lifted.block = Some(Block {
            until: Some("1970-01-02".into()),
            reason: None,
        });
        let f = of(&lifted, None);
        assert!(f.matches(&Chip::State(StateChip::Ready)));
        assert!(f.matches(&Chip::State(StateChip::NotBlocked)));
        let mut done = task("d");
        done.done = true;
        assert!(!of(&done, None).matches(&Chip::State(StateChip::NotBlocked)));
    }

    // Pins that chips in one group are alternatives and groups combine with AND.
    #[test]
    fn groups_and_chips_within_a_group_or() {
        let mut planned = task("a");
        planned.phase = Some("await-review".into());
        let f = of(&planned, Some(SessionState::Idle));
        assert!(passes(&f, &Filters::default()));
        assert!(passes(
            &f,
            &on(&[Chip::Phase(Some("await-review".into())), Chip::Phase(None)])
        ));
        assert!(passes(
            &f,
            &on(&[
                Chip::Phase(Some("await-review".into())),
                Chip::Session(SessionChip::Idle)
            ])
        ));
        assert!(!passes(
            &f,
            &on(&[
                Chip::Phase(Some("await-review".into())),
                Chip::Session(SessionChip::Busy)
            ])
        ));
    }

    // Pins that the Phase group offers every phase in use plus any that is on, "no phase" last.
    #[test]
    fn phase_candidates_come_from_the_data() {
        let mut a = task("a");
        a.phase = Some("implement".into());
        let mut b = task("b");
        b.phase = Some("await-fix".into());
        let items = vec![of(&a, None), of(&b, None), of(&task("c"), None)];
        let filters = on(&[Chip::Phase(Some("gone".into()))]);
        assert_eq!(
            candidates(Group::Phase, &items, &filters),
            vec![
                Chip::Phase(Some("await-fix".into())),
                Chip::Phase(Some("gone".into())),
                Chip::Phase(Some("implement".into())),
                Chip::Phase(None),
            ]
        );
    }

    // Pins a chip's count: its own group replaced by it, every other group applied.
    #[test]
    fn count_replaces_the_chips_own_group() {
        let mut a = task("a");
        a.repos[0].git.dirty = 1;
        let items = vec![
            of(&a, Some(SessionState::Busy)),
            of(&task("b"), Some(SessionState::Idle)),
            of(&task("c"), None),
        ];
        let filters = on(&[Chip::Session(SessionChip::Idle)]);
        assert_eq!(
            count(&items, &filters, &Chip::Session(SessionChip::Busy)),
            1
        );
        assert_eq!(count(&items, &filters, &Chip::Dirty), 0);
        assert_eq!(count(&items, &Filters::default(), &Chip::Dirty), 1);
    }
}
