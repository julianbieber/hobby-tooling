//! Which piece of work a tab is about: a task, or one aspect of it, located by the worktree the
//! pane runs in.

use std::path::{Path, PathBuf};

use hwt_core::Roots;

/// A task, or an aspect of one when `aspect` is set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub project: String,
    pub topic: String,
    pub task: String,
    pub aspect: Option<String>,
}

impl Scope {
    /// The scope of a directory inside an aspect worktree
    /// (`<aspects>/<project>/<topic>/<task>/<name>`) or a task worktree
    /// (`<worktrees>/<project>/<topic>/<task>`). `None` for any other directory.
    pub fn of_dir(roots: &Roots, dir: &Path) -> Option<Self> {
        if let Ok(rel) = dir.strip_prefix(&roots.aspects) {
            let [project, topic, task, name] = components::<4>(rel)?;
            return Some(Self {
                project,
                topic,
                task,
                aspect: Some(name),
            });
        }
        let rel = dir.strip_prefix(&roots.worktrees).ok()?;
        let [project, topic, task] = components::<3>(rel)?;
        Some(Self {
            project,
            topic,
            task,
            aspect: None,
        })
    }

    /// The scope named by `<project>/<topic>/<task>` or `<project>/<topic>/<task>/<aspect>`.
    pub fn parse(spec: &str) -> Option<Self> {
        let parts: Vec<&str> = spec.split('/').filter(|p| !p.is_empty()).collect();
        match parts.as_slice() {
            [project, topic, task] => Some(Self {
                project: (*project).to_owned(),
                topic: (*topic).to_owned(),
                task: (*task).to_owned(),
                aspect: None,
            }),
            [project, topic, task, aspect] => Some(Self {
                project: (*project).to_owned(),
                topic: (*topic).to_owned(),
                task: (*task).to_owned(),
                aspect: Some((*aspect).to_owned()),
            }),
            _ => None,
        }
    }

    pub fn task_notes(&self, roots: &Roots) -> PathBuf {
        roots
            .notes
            .join(&self.project)
            .join(&self.topic)
            .join(&self.task)
    }

    /// Where this scope's own records live: the aspect's notes directory for an aspect, else the
    /// task's.
    pub fn notes(&self, roots: &Roots) -> PathBuf {
        let task = self.task_notes(roots);
        match &self.aspect {
            Some(name) => task.join("aspects").join(name),
            None => task,
        }
    }

    /// The name the tab is known by: the aspect's name, else the task's.
    pub fn name(&self) -> &str {
        self.aspect.as_deref().unwrap_or(&self.task)
    }
}

fn components<const N: usize>(rel: &Path) -> Option<[String; N]> {
    let parts: Vec<String> = rel
        .components()
        .take(N)
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.try_into().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roots() -> Roots {
        Roots::under(Path::new("/h"))
    }

    #[test]
    fn an_aspect_worktree_and_its_subdirs_name_the_aspect() {
        let want = Scope {
            project: "p".into(),
            topic: "t".into(),
            task: "task".into(),
            aspect: Some("a".into()),
        };
        assert_eq!(
            Scope::of_dir(&roots(), Path::new("/h/fun_aspects/p/t/task/a")),
            Some(want.clone())
        );
        assert_eq!(
            Scope::of_dir(&roots(), Path::new("/h/fun_aspects/p/t/task/a/src")),
            Some(want.clone())
        );
        assert_eq!(
            want.notes(&roots()),
            Path::new("/h/fun_repos/hobby-mimisbrunnr/notes/p/t/task/aspects/a")
        );
        assert_eq!(want.name(), "a");
    }

    #[test]
    fn a_task_worktree_names_the_task() {
        let got = Scope::of_dir(&roots(), Path::new("/h/fun_worktrees/p/t/task/tools")).unwrap();
        assert_eq!(got.aspect, None);
        assert_eq!(got.name(), "task");
        assert_eq!(
            got.notes(&roots()),
            Path::new("/h/fun_repos/hobby-mimisbrunnr/notes/p/t/task")
        );
    }

    #[test]
    fn other_directories_have_no_scope() {
        assert_eq!(Scope::of_dir(&roots(), Path::new("/h/fun_repos/p")), None);
        assert_eq!(
            Scope::of_dir(&roots(), Path::new("/h/fun_worktrees/p/t")),
            None
        );
        assert_eq!(Scope::parse("p/t"), None);
        assert_eq!(Scope::parse("p/t/task/a").unwrap().name(), "a");
    }
}
