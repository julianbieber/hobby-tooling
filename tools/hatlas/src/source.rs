//! Reading a workspace's Rust source for the Bevy registration calls in it, by syntax
//! alone: which calls exist, what they name, and what is chained onto each name.

use crate::bevy::BevyNames;
use proc_macro2::LineColumn;
use std::path::Path;
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Expr, Type};

/// A name written in source, with where to look it up and how it was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ident {
    pub file: String,
    /// One-based.
    pub line: usize,
    /// Zero-based, in characters.
    pub col: usize,
    pub name: String,
    pub text: String,
}

/// What a registration names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Named(Ident),
    /// `observes` is the event type of an observer closure's `On<E>` first parameter, as written.
    Closure {
        file: String,
        line: usize,
        col: usize,
        observes: Option<Ident>,
    },
    Unparsed(String),
}

/// One thing a call registers, with everything chained onto it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub target: Target,
    pub conditions: Vec<String>,
    pub sets: Vec<Ident>,
    pub ordering: Vec<String>,
}

/// What a call sits inside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    /// `impl Plugin for X`, named by `X`.
    Plugin(Ident),
    Function(Ident),
    Other(String),
}

/// What a call registers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallKind {
    Systems {
        schedule: String,
        entries: Vec<Entry>,
    },
    Sets {
        schedule: String,
        entries: Vec<Entry>,
    },
    Observer(Vec<Entry>),
    Plugins(Vec<Entry>),
    Resource(Entry),
    /// A call that raises an event; `segments` is every path segment of the event expression, empty when it is not a path, struct literal or call.
    Produce {
        method: String,
        segments: Vec<Ident>,
        text: String,
    },
}

/// One registration call and where it was made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub owner: Option<Owner>,
    pub file: String,
    pub line: usize,
    pub kind: CallKind,
}

/// Parses `files` (relative to `root`) and returns every registration call in them,
/// plus one note for each file that could not be read or parsed.
///
/// Matches by method name only, whatever the receiver is.
pub fn read(root: &Path, files: &[String]) -> (Vec<Call>, Vec<String>) {
    let mut calls = Vec::new();
    let mut notes = Vec::new();
    for file in files {
        let text = match std::fs::read_to_string(root.join(file)) {
            Ok(t) => t,
            Err(e) => {
                notes.push(format!("{file} — unreadable: {e}"));
                continue;
            }
        };
        match read_source(file, &text) {
            Ok(found) => calls.extend(found),
            Err(e) => notes.push(format!("{file} — not parsed: {e}")),
        }
    }
    (calls, notes)
}

/// The calls in one file's text; `file` is only the name they are recorded under.
pub fn read_source(file: &str, text: &str) -> Result<Vec<Call>, syn::Error> {
    let parsed = syn::parse_file(text)?;
    let mut reader = Reader {
        file: file.to_string(),
        lines: text.lines().map(|l| l.chars().collect()).collect(),
        owners: Vec::new(),
        calls: Vec::new(),
    };
    reader.visit_file(&parsed);
    Ok(reader.calls)
}

struct Reader {
    file: String,
    lines: Vec<Vec<char>>,
    owners: Vec<Owner>,
    calls: Vec<Call>,
}

impl Reader {
    fn slice(&self, start: LineColumn, end: LineColumn) -> String {
        let mut out = String::new();
        for line in start.line..=end.line {
            let Some(chars) = self.lines.get(line - 1) else {
                break;
            };
            let from = if line == start.line { start.column } else { 0 };
            let to = if line == end.line {
                end.column
            } else {
                chars.len()
            };
            out.extend(chars.iter().skip(from).take(to.saturating_sub(from)));
            out.push(' ');
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn text(&self, node: &impl Spanned) -> String {
        let span = node.span();
        self.slice(span.start(), span.end())
    }

    fn ident_of_ident(&self, ident: &syn::Ident, text: String) -> Ident {
        let start = ident.span().start();
        Ident {
            file: self.file.clone(),
            line: start.line,
            col: start.column,
            name: ident.to_string(),
            text,
        }
    }

    fn ident_of_path(&self, path: &syn::Path) -> Option<Ident> {
        let last = path.segments.last()?;
        Some(self.ident_of_ident(&last.ident, self.text(path)))
    }

    fn ident_of_expr(&self, expr: &Expr) -> Ident {
        match strip(expr) {
            Expr::Path(p) => self
                .ident_of_path(&p.path)
                .unwrap_or_else(|| self.opaque(expr)),
            _ => self.opaque(expr),
        }
    }

    fn opaque(&self, expr: &Expr) -> Ident {
        let start = expr.span().start();
        let text = self.text(expr);
        Ident {
            file: self.file.clone(),
            line: start.line,
            col: start.column,
            name: text.clone(),
            text,
        }
    }

    fn schedule_of(&self, expr: &Expr) -> String {
        match strip(expr) {
            Expr::Path(p) if p.qself.is_none() => p
                .path
                .segments
                .last()
                .map_or_else(|| self.text(expr), |s| s.ident.to_string()),
            _ => self.text(expr),
        }
    }

    fn entries(&self, expr: &Expr) -> Vec<Entry> {
        match expr {
            Expr::Paren(p) => self.entries(&p.expr),
            Expr::Group(g) => self.entries(&g.expr),
            Expr::Reference(r) => self.entries(&r.expr),
            Expr::Tuple(t) => t.elems.iter().flat_map(|e| self.entries(e)).collect(),
            Expr::Path(p) => match self.ident_of_path(&p.path) {
                Some(id) => vec![entry(Target::Named(id))],
                None => vec![entry(Target::Unparsed(self.text(expr)))],
            },
            Expr::Struct(s) => match self.ident_of_path(&s.path) {
                Some(id) => vec![entry(Target::Named(id))],
                None => vec![entry(Target::Unparsed(self.text(expr)))],
            },
            Expr::Closure(c) => {
                let start = c.span().start();
                let observes = match c.inputs.first() {
                    Some(syn::Pat::Type(pt)) => self.observed_event(&pt.ty),
                    _ => None,
                };
                vec![entry(Target::Closure {
                    file: self.file.clone(),
                    line: start.line,
                    col: start.column,
                    observes,
                })]
            }
            Expr::Call(c) => match strip(&c.func) {
                Expr::Path(p) => {
                    let segments: Vec<_> = p.path.segments.iter().collect();
                    let pick = segments
                        .iter()
                        .rev()
                        .find(|s| s.ident != "default" && s.ident != "new");
                    match pick {
                        Some(seg) => vec![entry(Target::Named(
                            self.ident_of_ident(&seg.ident, self.text(expr)),
                        ))],
                        None => vec![entry(Target::Unparsed(self.text(expr)))],
                    }
                }
                _ => vec![entry(Target::Unparsed(self.text(expr)))],
            },
            Expr::MethodCall(m) => self.method_entries(m),
            _ => vec![entry(Target::Unparsed(self.text(expr)))],
        }
    }

    fn observed_event(&self, ty: &Type) -> Option<Ident> {
        let Type::Path(p) = ty else { return None };
        let last = p.path.segments.last()?;
        if !BevyNames::OBSERVER_PARAMS.contains(&last.ident.to_string().as_str()) {
            return None;
        }
        let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
            return None;
        };
        match args.args.first()? {
            syn::GenericArgument::Type(Type::Path(inner)) => self.ident_of_path(&inner.path),
            _ => None,
        }
    }

    fn event_segments(&self, arg: &Expr) -> Vec<Ident> {
        let path = match strip(arg) {
            Expr::Path(p) => &p.path,
            Expr::Struct(s) => &s.path,
            Expr::Call(c) => match strip(&c.func) {
                Expr::Path(p) => &p.path,
                _ => return Vec::new(),
            },
            _ => return Vec::new(),
        };
        path.segments
            .iter()
            .map(|s| self.ident_of_ident(&s.ident, s.ident.to_string()))
            .collect()
    }

    fn method_entries(&self, m: &syn::ExprMethodCall) -> Vec<Entry> {
        let name = m.method.to_string();
        let arg = m.args.first();
        if BevyNames::CONDITIONS.contains(&name.as_str()) {
            let mut found = self.entries(&m.receiver);
            if let Some(arg) = arg {
                let text = self.text(arg);
                found
                    .iter_mut()
                    .for_each(|e| e.conditions.push(text.clone()));
            }
            return found;
        }
        if name == BevyNames::IN_SET {
            let mut found = self.entries(&m.receiver);
            if let Some(arg) = arg {
                let set = self.ident_of_expr(arg);
                found.iter_mut().for_each(|e| e.sets.push(set.clone()));
            }
            return found;
        }
        let relation = if BevyNames::AFTER.contains(&name.as_str()) {
            Some("after")
        } else if BevyNames::BEFORE.contains(&name.as_str()) {
            Some("before")
        } else {
            None
        };
        if let (Some(relation), Some(arg)) = (relation, arg) {
            let mut found = self.entries(&m.receiver);
            let text = format!("{relation} {}", self.text(arg));
            found.iter_mut().for_each(|e| e.ordering.push(text.clone()));
            return found;
        }
        if BevyNames::CHAINS.contains(&name.as_str())
            && let Expr::Tuple(t) = strip(&m.receiver)
        {
            return self.chained(t);
        }
        self.entries(&m.receiver)
    }

    fn chained(&self, tuple: &syn::ExprTuple) -> Vec<Entry> {
        let members: Vec<(String, Vec<Entry>)> = tuple
            .elems
            .iter()
            .map(|e| (self.text(e), self.entries(e)))
            .collect();
        let mut out = Vec::new();
        for (i, (_, found)) in members.iter().enumerate() {
            for e in found {
                let mut e = e.clone();
                if let Some((previous, _)) = i.checked_sub(1).and_then(|p| members.get(p)) {
                    e.ordering.push(format!("after {previous}"));
                }
                if let Some((next, _)) = members.get(i + 1) {
                    e.ordering.push(format!("before {next}"));
                }
                out.push(e);
            }
        }
        out
    }

    fn push(&mut self, site: &syn::Ident, kind: CallKind) {
        self.calls.push(Call {
            owner: self.owners.last().cloned(),
            file: self.file.clone(),
            line: site.span().start().line,
            kind,
        });
    }
}

fn entry(target: Target) -> Entry {
    Entry {
        target,
        conditions: Vec::new(),
        sets: Vec::new(),
        ordering: Vec::new(),
    }
}

fn strip(expr: &Expr) -> &Expr {
    match expr {
        Expr::Paren(p) => strip(&p.expr),
        Expr::Group(g) => strip(&g.expr),
        other => other,
    }
}

fn self_type_ident(ty: &Type) -> Option<&syn::Ident> {
    match ty {
        Type::Path(p) => p.path.segments.last().map(|s| &s.ident),
        _ => None,
    }
}

impl<'ast> Visit<'ast> for Reader {
    fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
        let owner = Owner::Function(self.ident_of_ident(&f.sig.ident, f.sig.ident.to_string()));
        self.owners.push(owner);
        syn::visit::visit_item_fn(self, f);
        self.owners.pop();
    }

    fn visit_item_impl(&mut self, imp: &'ast syn::ItemImpl) {
        let is_plugin = imp
            .trait_
            .as_ref()
            .and_then(|(_, p, _)| p.segments.last())
            .is_some_and(|s| s.ident == BevyNames::PLUGIN);
        let ty = self_type_ident(&imp.self_ty);
        for item in &imp.items {
            let syn::ImplItem::Fn(f) = item else { continue };
            let owner = match (is_plugin, ty) {
                (true, Some(ty)) => Owner::Plugin(self.ident_of_ident(ty, ty.to_string())),
                (_, Some(ty)) => Owner::Other(format!("{ty}::{}", f.sig.ident)),
                _ => Owner::Other(f.sig.ident.to_string()),
            };
            self.owners.push(owner);
            syn::visit::visit_impl_item_fn(self, f);
            self.owners.pop();
        }
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        let name = m.method.to_string();
        let args: Vec<&Expr> = m.args.iter().collect();
        if name == BevyNames::ADD_SYSTEMS || BevyNames::CONFIGURE_SETS.contains(&name.as_str()) {
            let (schedule, cfg) = match args.as_slice() {
                [schedule, cfg] => (self.schedule_of(schedule), Some(*cfg)),
                [cfg] => (String::new(), Some(*cfg)),
                _ => (String::new(), None),
            };
            if let Some(cfg) = cfg {
                let entries = self.entries(cfg);
                let kind = if name == BevyNames::ADD_SYSTEMS {
                    CallKind::Systems { schedule, entries }
                } else {
                    CallKind::Sets { schedule, entries }
                };
                self.push(&m.method, kind);
            }
        } else if BevyNames::ADD_OBSERVER.contains(&name.as_str()) {
            if let Some(arg) = args.first() {
                let entries = self.entries(arg);
                self.push(&m.method, CallKind::Observer(entries));
            }
        } else if BevyNames::ADD_PLUGINS.contains(&name.as_str()) {
            if let Some(arg) = args.first() {
                let entries = self.entries(arg);
                self.push(&m.method, CallKind::Plugins(entries));
            }
        } else if BevyNames::RESOURCE_CALLS.contains(&name.as_str()) {
            let target = m
                .turbofish
                .as_ref()
                .and_then(|t| t.args.first())
                .and_then(|a| match a {
                    syn::GenericArgument::Type(Type::Path(p)) => self.ident_of_path(&p.path),
                    _ => None,
                })
                .map(Target::Named)
                .or_else(|| {
                    args.first()
                        .and_then(|a| self.entries(a).into_iter().next())
                        .map(|e| e.target)
                });
            if let Some(target) = target {
                self.push(&m.method, CallKind::Resource(entry(target)));
            }
        } else if BevyNames::PRODUCING_CALLS.contains(&name.as_str())
            && let Some(arg) = args.first()
        {
            let segments = self.event_segments(arg);
            let text = self.text(*arg);
            self.push(
                &m.method,
                CallKind::Produce {
                    method: name.clone(),
                    segments,
                    text,
                },
            );
        }
        syn::visit::visit_expr_method_call(self, m);
    }

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let Expr::Path(p) = strip(&c.func)
            && let Some(last) = p.path.segments.last()
            && BevyNames::ADD_OBSERVER.contains(&last.ident.to_string().as_str())
            && let Some(arg) = c.args.first()
        {
            let entries = self.entries(arg);
            self.push(&last.ident, CallKind::Observer(entries));
        }
        syn::visit::visit_expr_call(self, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calls(src: &str) -> Vec<Call> {
        read_source("a.rs", src).unwrap()
    }

    fn systems(c: &Call) -> &[Entry] {
        match &c.kind {
            CallKind::Systems { entries, .. } | CallKind::Sets { entries, .. } => entries,
            other => panic!("{other:?}"),
        }
    }

    fn name(e: &Entry) -> &str {
        match &e.target {
            Target::Named(i) => &i.name,
            other => panic!("{other:?}"),
        }
    }

    // Pins the wusel shape: a chained tuple of sets with one run condition gives every member the condition and its neighbours as ordering.
    #[test]
    fn chained_sets_spread_condition_and_ordering() {
        let c = calls(
            "fn build(app: &mut App) { app.configure_sets(Update, (A::One, A::Two, A::Three).chain().run_if(in_state(S::G))); }",
        );
        let CallKind::Sets { schedule, entries } = &c[0].kind else {
            panic!()
        };
        assert_eq!(schedule, "Update");
        assert_eq!(
            entries.iter().map(name).collect::<Vec<_>>(),
            ["One", "Two", "Three"]
        );
        assert!(entries.iter().all(|e| e.conditions == ["in_state(S::G)"]));
        assert_eq!(entries[1].ordering, ["after A::One", "before A::Three"]);
        assert_eq!(entries[0].ordering, ["before A::Two"]);
    }

    // Pins that in_set on a chained tuple reaches each member, as the fixture's registration relies on.
    #[test]
    fn in_set_applies_to_every_member() {
        let c = calls("fn f() { app.add_systems(Update, (a, b).chain().in_set(Sys::Run)); }");
        let e = systems(&c[0]);
        assert_eq!(e.len(), 2);
        assert!(
            e.iter()
                .all(|e| e.sets.len() == 1 && e.sets[0].name == "Run")
        );
        assert_eq!(e[1].ordering, ["after a"]);
    }

    // Pins that a closure is kept as an anonymous system with its position, not dropped or confused with a path.
    #[test]
    fn closure_is_an_anonymous_target() {
        let c = calls("fn f() { app.add_systems(Update, |x: Res<R>| {}); }");
        assert!(matches!(
            systems(&c[0])[0].target,
            Target::Closure { line: 1, .. }
        ));
    }

    // Pins that a trigger call keeps every segment of its event expression, so the resolver can find the type before a variant.
    #[test]
    fn produce_calls_keep_event_segments() {
        let names = |src: &str| match &calls(src)[0].kind {
            CallKind::Produce { segments, .. } => {
                segments.iter().map(|s| s.name.clone()).collect::<Vec<_>>()
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(names("fn f() { commands.trigger(Foo { a: 1 }); }"), ["Foo"]);
        assert_eq!(
            names("fn f() { world.write_message(AppExit::Success); }"),
            ["AppExit", "Success"]
        );
        assert!(names("fn f() { w.send(x_y()); }").len() == 1);
        assert!(names("fn f() { w.send(1 + 2); }").is_empty());
    }

    // Pins the free-function `observe(closure)` form and that the closure records the event its `On<E>` parameter names.
    #[test]
    fn observe_call_names_its_event() {
        let c = calls("fn f() { row.spawn(observe(|_: On<Click>, q: Res<R>| {})); }");
        let CallKind::Observer(e) = &c[0].kind else {
            panic!("{:?}", c[0].kind)
        };
        let Target::Closure { observes, .. } = &e[0].target else {
            panic!()
        };
        assert_eq!(observes.as_ref().map(|i| i.name.as_str()), Some("Click"));
    }

    // Pins that a closure whose first parameter is not an observer parameter observes nothing.
    #[test]
    fn plain_closure_observes_nothing() {
        let c = calls("fn f() { app.add_observer(|x: Res<R>| {}); }");
        let CallKind::Observer(e) = &c[0].kind else {
            panic!()
        };
        assert!(matches!(
            &e[0].target,
            Target::Closure { observes: None, .. }
        ));
    }

    // Pins that run_if on one tuple member does not leak onto its siblings.
    #[test]
    fn condition_stays_on_its_member() {
        let c = calls("fn f() { app.add_systems(Update, (a.run_if(x), b)); }");
        let e = systems(&c[0]);
        assert_eq!(e[0].conditions, ["x"]);
        assert!(e[1].conditions.is_empty());
    }

    // Pins that a macro in a registration is reported as unparsed rather than guessed at.
    #[test]
    fn macro_target_is_unparsed() {
        let c = calls("fn f() { app.add_systems(Update, gen!()); }");
        assert!(matches!(systems(&c[0])[0].target, Target::Unparsed(_)));
    }

    // Pins that calls inside `impl Plugin` are owned by the implementing type, and that a factory call resolves to the factory's name.
    #[test]
    fn plugin_impl_owns_its_calls() {
        let c = calls(
            "impl Plugin for P { fn build(&self, app: &mut App) { app.add_plugins(Other::default()); app.init_resource::<R>(); } }",
        );
        assert!(matches!(&c[0].owner, Some(Owner::Plugin(i)) if i.name == "P"));
        let CallKind::Plugins(e) = &c[0].kind else {
            panic!()
        };
        assert_eq!(name(&e[0]), "Other");
        assert!(matches!(&c[1].kind, CallKind::Resource(r) if name(r) == "R"));
    }
}
