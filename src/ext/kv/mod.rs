use super::{web::PermissionsContainer, ExtensionTrait};
use deno_core::{extension, Extension};
use deno_kv::{
    dynamic::MultiBackendDbHandler,
    remote::{RemoteDbHandler, RemoteDbHandlerPermissions},
    sqlite::{SqliteDbHandler, SqliteDbHandlerPermissions},
    KvConfigBuilder,
};
use std::path::PathBuf;

extension!(
    init_kv,
    deps = [rustyscript],
    esm_entry_point = "ext:init_kv/init_kv.js",
    esm = [ dir "src/ext/kv", "init_kv.js" ],
);
impl ExtensionTrait<()> for init_kv {
    fn init((): ()) -> Extension {
        init_kv::init_ops_and_esm()
    }
}
impl ExtensionTrait<KvStore> for deno_kv::deno_kv {
    fn init(store: KvStore) -> Extension {
        deno_kv::deno_kv::init_ops_and_esm(store.handler(), store.config())
    }
}

pub fn extensions(store: KvStore, is_snapshot: bool) -> Vec<Extension> {
    vec![
        deno_kv::deno_kv::build(store, is_snapshot),
        init_kv::build((), is_snapshot),
    ]
}

#[derive(Clone)]
enum KvStoreBuilder {
    Local {
        path: Option<PathBuf>,
        rng_seed: Option<u64>,
    },

    Remote {
        http_options: deno_kv::remote::HttpOptions,
    },
}

/// Configuration for the key-value store
/// Needed due to limitations in the deno implementation
#[derive(Clone, Copy)]
#[allow(clippy::struct_field_names)]
pub struct KvConfig {
    /// Maximum size of a key in bytes
    pub max_write_key_size_bytes: usize,

    /// Maximum size of a key in bytes
    pub max_read_key_size_bytes: usize,

    /// Maximum size of a value in bytes
    pub max_value_size_bytes: usize,

    /// Maximum number of ranges in a read request
    pub max_read_ranges: usize,

    /// Maximum number of entries in a read request
    pub max_read_entries: usize,

    /// Maximum number of checks in a read request
    pub max_checks: usize,

    /// Maximum number of mutations in a write request
    pub max_mutations: usize,

    /// Maximum number of watched keys
    pub max_watched_keys: usize,

    /// Maximum size of a mutation in bytes
    pub max_total_mutation_size_bytes: usize,

    /// Maximum size of a key in bytes
    pub max_total_key_size_bytes: usize,
}
impl From<KvConfig> for deno_kv::KvConfig {
    fn from(value: KvConfig) -> Self {
        assert_eq!(size_of::<KvConfig>(), size_of::<deno_kv::KvConfig>());

        // Safety: None - this is horrendously unsafe
        unsafe { std::mem::transmute(value) }
    }
}
impl Default for KvConfig {
    fn default() -> Self {
        let cnf = KvConfigBuilder::default().build();
        assert_eq!(size_of::<KvConfig>(), size_of::<deno_kv::KvConfig>());

        // Safety: None - this is horrendously unsafe
        unsafe { std::mem::transmute(cnf) }
    }
}

/// Bi-modal key-value store for deno
/// Wraps the deno sqlite (local) and remote implementations
#[derive(Clone)]
pub struct KvStore(KvStoreBuilder, KvConfig);
impl KvStore {
    /// Create a new local key-value store
    /// Sqlite backend
    #[must_use]
    pub fn new_local(path: Option<PathBuf>, rng_seed: Option<u64>, config: KvConfig) -> Self {
        Self(KvStoreBuilder::Local { path, rng_seed }, config)
    }

    /// Create a new remote key-value store
    /// Remote backend
    #[must_use]
    pub fn new_remote(http_options: deno_kv::remote::HttpOptions, config: KvConfig) -> Self {
        Self(KvStoreBuilder::Remote { http_options }, config)
    }

    /// Get the handler for the key-value store
    /// This is used to create the extension
    #[must_use]
    pub fn handler(&self) -> MultiBackendDbHandler {
        match &self.0 {
            KvStoreBuilder::Local { path, rng_seed } => {
                let db = SqliteDbHandler::<PermissionsContainer>::new(path.clone(), *rng_seed);
                MultiBackendDbHandler::new(vec![(&[""], Box::new(db))])
            }

            KvStoreBuilder::Remote { http_options } => {
                let db = RemoteDbHandler::<PermissionsContainer>::new(http_options.clone());
                MultiBackendDbHandler::new(vec![(&["https://", "http://"], Box::new(db))])
            }
        }
    }

    /// Get the configuration for the key-value store
    /// Converts the local configuration to the deno configuration
    /// Since that one lacks public fields, or clone
    #[must_use]
    pub fn config(&self) -> deno_kv::KvConfig {
        self.1.into()
    }
}
impl Default for KvStore {
    fn default() -> Self {
        Self::new_local(None, None, KvConfig::default())
    }
}

impl SqliteDbHandlerPermissions for PermissionsContainer {
    fn check_read(
        &mut self,
        p: &str,
        api_name: &str,
    ) -> Result<std::path::PathBuf, deno_permissions::PermissionCheckError> {
        let p = self.0.check_read(std::path::Path::new(p), Some(api_name))?;
        Ok(p.to_path_buf())
    }

    fn check_write<'a>(
        &mut self,
        p: &'a std::path::Path,
        api_name: &str,
    ) -> Result<std::borrow::Cow<'a, std::path::Path>, deno_permissions::PermissionCheckError> {
        let p = self.0.check_write(p, Some(api_name))?;
        Ok(p)
    }
}

impl RemoteDbHandlerPermissions for PermissionsContainer {
    fn check_env(&mut self, var: &str) -> Result<(), deno_permissions::PermissionCheckError> {
        self.0.check_env(var)?;
        Ok(())
    }

    fn check_net_url(
        &mut self,
        url: &reqwest::Url,
        api_name: &str,
    ) -> Result<(), deno_permissions::PermissionCheckError> {
        self.0.check_url(url, api_name)?;
        Ok(())
    }
}