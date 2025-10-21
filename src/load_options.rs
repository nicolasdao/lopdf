/// Progress information during PDF loading
#[derive(Debug, Clone)]
pub struct LoadProgress {
    /// Current stage (0-6)
    pub stage: u8,

    /// Name of current stage
    pub stage_name: &'static str,

    /// Overall progress (0.0 to 1.0)
    pub progress: f64,

    /// Number of items processed in current stage
    pub items_processed: usize,

    /// Total items in current stage
    pub items_total: usize,

    /// Optional message
    pub message: Option<String>,
}

impl LoadProgress {
    /// Create a new LoadProgress
    pub fn new(
        stage: u8,
        progress: f64,
        items_processed: usize,
        items_total: usize,
        message: Option<String>,
    ) -> Self {
        let stage_name = match stage {
            0 => "Reading file",
            1 => "Finding PDF header",
            2 => "Parsing version",
            3 => "Parsing cross-reference table",
            4 => "Parsing trailer",
            5 => "Loading objects",
            6 => "Complete",
            _ => "Unknown",
        };

        Self {
            stage,
            stage_name,
            progress,
            items_processed,
            items_total,
            message,
        }
    }
}

/// Interval for progress callbacks
#[derive(Debug, Clone, Copy)]
pub enum ProgressInterval {
    /// Report every N items
    Items(usize),

    /// Report every N percent (0.0 to 100.0)
    Percentage(f64),
}

impl Default for ProgressInterval {
    fn default() -> Self {
        Self::Percentage(1.0)
    }
}

/// Options for loading PDF documents
pub struct LoadOptions<'a> {
    /// Optional progress callback
    pub progress_callback: Option<Box<dyn Fn(LoadProgress) + Send + Sync + 'a>>,

    /// Progress reporting interval
    pub progress_interval: ProgressInterval,
}

impl Default for LoadOptions<'_> {
    fn default() -> Self {
        Self {
            progress_callback: None,
            progress_interval: ProgressInterval::default(),
        }
    }
}

impl<'a> LoadOptions<'a> {
    /// Create a new LoadOptions with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a progress callback
    ///
    /// The callback can now borrow from the local scope, enabling real-time progress updates.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lopdf::LoadOptions;
    ///
    /// let options = LoadOptions::new()
    ///     .with_progress(|p| {
    ///         println!("{}%: {}", (p.progress * 100.0) as u8, p.stage_name);
    ///     });
    /// ```
    ///
    /// ```rust
    /// use lopdf::LoadOptions;
    /// use std::sync::Arc;
    /// use std::sync::Mutex;
    ///
    /// // Can now borrow from local scope!
    /// let progress_counter = Arc::new(Mutex::new(0));
    /// let counter_ref = Arc::clone(&progress_counter);
    ///
    /// let options = LoadOptions::new()
    ///     .with_progress(move |p| {
    ///         *counter_ref.lock().unwrap() += 1;
    ///         println!("{}%", (p.progress * 100.0) as u8);
    ///     });
    /// ```
    pub fn with_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(LoadProgress) + Send + Sync + 'a,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Set the progress reporting interval
    ///
    /// # Examples
    ///
    /// ```rust
    /// use lopdf::{LoadOptions, ProgressInterval};
    ///
    /// // Report every 10 items
    /// let options = LoadOptions::new()
    ///     .with_progress_interval(ProgressInterval::Items(10));
    ///
    /// // Report every 5%
    /// let options = LoadOptions::new()
    ///     .with_progress_interval(ProgressInterval::Percentage(5.0));
    /// ```
    pub fn with_progress_interval(mut self, interval: ProgressInterval) -> Self {
        self.progress_interval = interval;
        self
    }
}
