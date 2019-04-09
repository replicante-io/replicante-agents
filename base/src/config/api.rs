use std::collections::HashMap;
use std::sync::RwLock;


// Define some globals to hold the default overrides.
lazy_static! {
    static ref DEFAULT_BIND: RwLock<Option<String>> = RwLock::new(None);
}


/// Web server configuration options.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct APIConfig {
    /// Local addess to bind the API server to.
    #[serde(default = "APIConfig::default_bind")]
    pub bind: String,

    /// Enable/disable entire API trees.
    #[serde(default)]
    pub trees: APITrees,
}

impl Default for APIConfig {
    fn default() -> Self {
        APIConfig {
            bind: Self::default_bind(),
            trees: APITrees::default(),
        }
    }
}

impl APIConfig {
    /// Default value for `bind` used by serde.
    fn default_bind() -> String {
        DEFAULT_BIND.read().unwrap()
            .as_ref().map(Clone::clone)
            .unwrap_or_else(|| String::from("127.0.0.1:8000"))
    }
}

impl APIConfig {
    /// Overrides the default bind attribute.
    ///
    /// This should be done at the very beginning of your agent and
    /// BEFORE ANY CONFIGURATION IS LOADED/INSTANTIATED.
    ///
    /// # Panics
    /// If the default is set more then once.
    pub fn set_default_bind(bind: String) {
        let mut default = DEFAULT_BIND.write().unwrap();
        if default.is_some() {
            panic!("Cannot override the default api.bind option more than once");
        }
        *default = Some(bind);
    }
}

/// Enable/disable entire API trees.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub struct APITrees {
    /// Enable/disable the introspection APIs.
    #[serde(default = "APITrees::default_true")]
    pub introspect: bool,

    /// Enable/disable the unstable API.
    #[serde(default = "APITrees::default_true")]
    pub unstable: bool,
}

impl Default for APITrees {
    fn default() -> APITrees {
        APITrees {
            introspect: true,
            unstable: true,
        }
    }
}

impl APITrees {
    fn default_true() -> bool {
        true
    }
}

// We can's fulfill the wish of the implicit-hasher clippy because
// we do not use the genieric hasher parameter in any LOCAL type.
#[allow(clippy::implicit_hasher)]
impl From<APITrees> for HashMap<&'static str, bool> {
    fn from(trees: APITrees) -> HashMap<&'static str, bool> {
        let mut flags = HashMap::default();
        flags.insert("introspect", trees.introspect);
        flags.insert("unstable", trees.unstable);
        flags
    }
}
