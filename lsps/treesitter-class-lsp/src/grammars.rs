use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use tree_sitter::Language;

use crate::languages::{LanguageInfo, LanguageRegistry};

/// A grammar loaded from a helix `.so`. We keep the `Library` alive for the
/// process lifetime so the `Language` pointer inside remains valid.
pub struct LoadedGrammar {
    _library: libloading::Library,
    pub language: Language,
}

impl LoadedGrammar {
    fn load(path: &Path, symbol: &str) -> anyhow::Result<Self> {
        unsafe {
            let library = libloading::Library::new(path)
                .with_context(|| format!("loading {}", path.display()))?;
            type LangFn = unsafe extern "C" fn() -> Language;
            // Scope the Symbol borrow so we can move `library` into the struct.
            let language = {
                let get_lang: libloading::Symbol<LangFn> = library
                    .get(symbol.as_bytes())
                    .with_context(|| format!("symbol {} in {}", symbol, path.display()))?;
                get_lang()
            };
            Ok(Self {
                _library: library,
                language,
            })
        }
    }
}

/// Process-wide grammar cache. Loading a `.so` is slow-ish and the returned
/// `Language` is intended to be shared across threads.
#[derive(Default, Clone)]
pub struct GrammarCache {
    inner: Arc<Mutex<HashMap<String, Arc<LoadedGrammar>>>>,
}

impl GrammarCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_load(
        &self,
        registry: &LanguageRegistry,
        lang: &LanguageInfo,
    ) -> anyhow::Result<Arc<LoadedGrammar>> {
        {
            let guard = self.inner.lock().unwrap();
            if let Some(g) = guard.get(&lang.name) {
                return Ok(Arc::clone(g));
            }
        }
        let symbol = registry.symbol_name(lang);
        let grammar = LoadedGrammar::load(&lang.grammar_path, &symbol)?;
        let arc = Arc::new(grammar);
        let mut guard = self.inner.lock().unwrap();
        // Handle races: another thread may have inserted first.
        Ok(Arc::clone(guard.entry(lang.name.clone()).or_insert(arc)))
    }
}
