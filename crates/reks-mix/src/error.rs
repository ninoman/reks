//! Error type for `reks-mix`.

/// Errors returned by `reks-mix`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No output device by that name, or no default output device.
    #[error("no audio output device {0:?}")]
    NoDevice(Option<String>),

    /// The device does not offer the sample rate asked for.
    #[error("{1} does not support {0} Hz")]
    RateUnsupported(u32, String),

    /// The device does not take that buffer size, in frames.
    #[error("{1} does not support {0}-frame buffers")]
    BufferUnsupported(u32, String),

    /// The device's sample format is one we do not write.
    #[error("unsupported sample format {0} on {1}")]
    SampleFormat(String, String),

    /// cpal failed to list devices or open, start or query a stream.
    #[error("audio device error: {0}")]
    Device(#[from] cpal::Error),

    /// The control queue is full: the audio thread is not running.
    #[error("audio engine is not responding")]
    QueueFull,

    /// The track was decoded for another output rate.
    #[error("track is at {track} Hz but the output runs at {device} Hz")]
    RateMismatch { track: u32, device: u32 },

    /// The engine thread exited before reporting.
    #[error("audio engine thread failed to start")]
    ThreadFailed,
}

/// Result alias for `reks-mix`.
pub type Result<T> = std::result::Result<T, Error>;
