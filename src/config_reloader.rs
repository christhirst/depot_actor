/// Reusable Config Reloader Module
///
/// This module provides a generic, thread-safe configuration reloader that can be
/// triggered manually (e.g., via gRPC) or automatically via file watching.
///
/// # Features
/// - Generic over any config type that implements `Clone` and can be loaded
/// - Thread-safe using `Arc<RwLock<T>>`
/// - Change notifications via `tokio::sync::watch`
/// - Easy to copy to other projects
///
/// # Example
/// ```rust,ignore
/// use config_reloader::ConfigReloader;
///
/// // Define your config loader
/// fn load_config() -> Result<MyConfig, Box<dyn std::error::Error>> {
///     MyConfig::load()
/// }
///
/// // Create reloader
/// let reloader = ConfigReloader::new(load_config)?;
///
/// // Get current config
/// let config = reloader.get();
///
/// // Reload config
/// reloader.reload()?;
///
/// // Subscribe to changes
/// let mut rx = reloader.subscribe();
/// tokio::spawn(async move {
///     while rx.changed().await.is_ok() {
///         let new_config = rx.borrow().clone();
///         println!("Config changed!");
///     }
/// });
/// ```
use std::sync::{Arc, RwLock};
use tokio::sync::watch;

/// Generic configuration reloader
///
/// `T` is your configuration type (e.g., `Settings`)
/// `E` is the error type returned by your loader function
pub struct ConfigReloader<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    /// Current configuration wrapped in Arc<RwLock> for thread-safe access
    config: Arc<RwLock<T>>,

    /// Function to load/reload the configuration
    loader: Arc<dyn Fn() -> Result<T, E> + Send + Sync>,

    /// Watch channel sender for broadcasting config changes
    tx: watch::Sender<T>,

    /// Watch channel receiver (kept for cloning to subscribers)
    _rx: watch::Receiver<T>,
}

impl<T, E> ConfigReloader<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    /// Create a new ConfigReloader with the given loader function
    ///
    /// # Arguments
    /// * `loader` - Function that loads the configuration from disk/source
    ///
    /// # Returns
    /// * `Ok(ConfigReloader)` - Successfully created reloader with initial config loaded
    /// * `Err(E)` - Failed to load initial configuration
    pub fn new<F>(loader: F) -> Result<Self, E>
    where
        F: Fn() -> Result<T, E> + Send + Sync + 'static,
    {
        let initial_config = loader()?;
        let config = Arc::new(RwLock::new(initial_config.clone()));
        let (tx, rx) = watch::channel(initial_config);

        Ok(Self {
            config,
            loader: Arc::new(loader),
            tx,
            _rx: rx,
        })
    }

    /// Get a clone of the current configuration
    ///
    /// This is a cheap operation as it only clones the config, not the Arc/RwLock
    pub fn get(&self) -> T {
        self.config.read().unwrap().clone()
    }

    /// Get an Arc to the current configuration for shared ownership
    ///
    /// Useful when you want to avoid cloning and can work with Arc<RwLock<T>>
    pub fn get_arc(&self) -> Arc<RwLock<T>> {
        Arc::clone(&self.config)
    }

    /// Reload the configuration from source
    ///
    /// This will:
    /// 1. Call the loader function to get new config
    /// 2. Update the internal config
    /// 3. Notify all subscribers of the change
    ///
    /// # Returns
    /// * `Ok(())` - Successfully reloaded
    /// * `Err(E)` - Failed to load new configuration (old config remains)
    pub fn reload(&self) -> Result<(), E> {
        let new_config = (self.loader)()?;

        // Update the config
        {
            let mut config = self.config.write().unwrap();
            *config = new_config.clone();
        }

        // Notify subscribers (ignore error if no receivers)
        let _ = self.tx.send(new_config);

        Ok(())
    }

    /// Update configuration with a provided config value
    ///
    /// This will:
    /// 1. Update the internal config with the provided value
    /// 2. Notify all subscribers of the change
    ///
    /// Unlike `reload()`, this does not call the loader function.
    /// Useful for updating config from external sources (e.g., gRPC).
    ///
    /// # Returns
    /// * `Ok(())` - Successfully updated
    pub fn update(&self, new_config: T) -> Result<(), E> {
        // Update the config
        {
            let mut config = self.config.write().unwrap();
            *config = new_config.clone();
        }

        // Notify subscribers (ignore error if no receivers)
        let _ = self.tx.send(new_config);

        Ok(())
    }

    /// Subscribe to configuration changes
    ///
    /// Returns a watch::Receiver that will be notified whenever the config is reloaded
    ///
    /// # Example
    /// ```rust,ignore
    /// let mut rx = reloader.subscribe();
    /// tokio::spawn(async move {
    ///     while rx.changed().await.is_ok() {
    ///         let new_config = rx.borrow().clone();
    ///         // Handle config change
    ///     }
    /// });
    /// ```
    pub fn subscribe(&self) -> watch::Receiver<T> {
        self.tx.subscribe()
    }
}

impl<T, E> Clone for ConfigReloader<T, E>
where
    T: Clone + Send + Sync + 'static,
    E: std::error::Error + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            loader: Arc::clone(&self.loader),
            tx: self.tx.clone(),
            _rx: self.tx.subscribe(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct TestConfig {
        value: i32,
    }

    #[test]
    fn test_config_reloader_new() {
        let loader = || Ok::<TestConfig, std::io::Error>(TestConfig { value: 42 });
        let reloader = ConfigReloader::new(loader).unwrap();
        assert_eq!(reloader.get().value, 42);
    }

    #[test]
    fn test_config_reloader_reload() {
        let counter = Arc::new(RwLock::new(0));
        let counter_clone = Arc::clone(&counter);

        let loader = move || {
            let mut c = counter_clone.write().unwrap();
            *c += 1;
            Ok::<TestConfig, std::io::Error>(TestConfig { value: *c })
        };

        let reloader = ConfigReloader::new(loader).unwrap();
        assert_eq!(reloader.get().value, 1);

        reloader.reload().unwrap();
        assert_eq!(reloader.get().value, 2);

        reloader.reload().unwrap();
        assert_eq!(reloader.get().value, 3);
    }

    #[tokio::test]
    async fn test_config_reloader_subscribe() {
        let counter = Arc::new(RwLock::new(0));
        let counter_clone = Arc::clone(&counter);

        let loader = move || {
            let mut c = counter_clone.write().unwrap();
            *c += 1;
            Ok::<TestConfig, std::io::Error>(TestConfig { value: *c })
        };

        let reloader = ConfigReloader::new(loader).unwrap();
        let mut rx = reloader.subscribe();

        // Initial value
        assert_eq!(rx.borrow().value, 1);

        // Reload and check notification
        reloader.reload().unwrap();
        rx.changed().await.unwrap();
        assert_eq!(rx.borrow().value, 2);
    }
}
