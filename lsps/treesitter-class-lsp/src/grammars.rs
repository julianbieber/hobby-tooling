//! Loading Helix's compiled tree-sitter grammars and sharing them across threads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use tree_sitter::Language;

use crate::languages::{LanguageInfo, LanguageRegistry};

/// A grammar that could not be loaded from its shared library.
#[derive(Debug, thiserror::Error)]
pub enum GrammarError {
    #[error("cannot load grammar library {}", path.display())]
    Library {
        path: PathBuf,
        #[source]
        source: libloading::Error,
    },
    #[error("grammar library {} has no symbol {symbol}", path.display())]
    MissingSymbol {
        path: PathBuf,
        symbol: String,
        #[source]
        source: libloading::Error,
    },
}

/// A grammar loaded from a helix `.so`. The `Library` is held for as long as
/// the `Language` it produced, so the pointers inside it stay valid.
pub struct LoadedGrammar {
    pub language: Language,
    _library: libloading::Library,
}

impl LoadedGrammar {
    #[expect(
        unsafe_code,
        reason = "a tree-sitter grammar is a C shared library; dlopen and its language function are FFI"
    )]
    fn load(path: &Path, symbol: &str) -> Result<Self, GrammarError> {
        // SAFETY: the library is a tree-sitter grammar from helix's runtime, whose
        // initialisers have no preconditions.
        let library =
            unsafe { libloading::Library::new(path) }.map_err(|source| GrammarError::Library {
                path: path.to_path_buf(),
                source,
            })?;
        let language = {
            // SAFETY: every tree-sitter grammar exports `tree_sitter_<name>` with this
            // signature, returning a pointer into the library's static data.
            let get_language: libloading::Symbol<unsafe extern "C" fn() -> Language> = unsafe {
                library.get(symbol.as_bytes())
            }
            .map_err(|source| GrammarError::MissingSymbol {
                path: path.to_path_buf(),
                symbol: symbol.to_string(),
                source,
            })?;
            // SAFETY: the function takes no arguments and only returns that pointer;
            // `library` is moved into the result, so it outlives the language.
            unsafe { get_language() }
        };
        Ok(Self {
            language,
            _library: library,
        })
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
    ) -> Result<Arc<LoadedGrammar>, GrammarError> {
        if let Some(grammar) = self.inner.lock().get(&lang.name) {
            return Ok(Arc::clone(grammar));
        }
        let symbol = registry.symbol_name(lang);
        let grammar = Arc::new(LoadedGrammar::load(&lang.grammar_path, &symbol)?);
        Ok(Arc::clone(
            self.inner
                .lock()
                .entry(lang.name.clone())
                .or_insert(grammar),
        ))
    }
}
