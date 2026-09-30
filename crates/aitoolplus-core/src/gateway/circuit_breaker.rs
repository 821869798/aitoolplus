use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,   // Healthy, traffic allowed
    Open,     // Cooling down, requests should fail or skip
    HalfOpen, // Probing with one request
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub cooling_duration: Duration,
    pub half_open_success_threshold: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 3,
            cooling_duration: Duration::from_secs(30),
            half_open_success_threshold: 1,
        }
    }
}

#[derive(Debug)]
struct ProviderBreakerState {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_time: Option<Instant>,
    config: CircuitBreakerConfig,
}

impl ProviderBreakerState {
    fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            last_failure_time: None,
            config,
        }
    }

    fn check_state(&mut self) -> CircuitState {
        match self.state {
            CircuitState::Closed => CircuitState::Closed,
            CircuitState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() >= self.config.cooling_duration {
                        self.state = CircuitState::HalfOpen;
                        self.success_count = 0;
                        CircuitState::HalfOpen
                    } else {
                        CircuitState::Open
                    }
                } else {
                    self.state = CircuitState::Closed;
                    CircuitState::Closed
                }
            }
            CircuitState::HalfOpen => CircuitState::HalfOpen,
        }
    }

    fn record_success(&mut self) {
        match self.state {
            CircuitState::HalfOpen => {
                self.success_count += 1;
                if self.success_count >= self.config.half_open_success_threshold {
                    self.state = CircuitState::Closed;
                    self.failure_count = 0;
                    self.last_failure_time = None;
                }
            }
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::Open => {}
        }
    }

    fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());
        match self.state {
            CircuitState::Closed => {
                if self.failure_count >= self.config.failure_threshold {
                    self.state = CircuitState::Open;
                }
            }
            CircuitState::HalfOpen => {
                self.state = CircuitState::Open;
            }
            CircuitState::Open => {}
        }
    }

    fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.success_count = 0;
        self.last_failure_time = None;
    }
}

/// Global circuit breaker registry for managing provider health.
pub struct CircuitBreakerRegistry {
    states: RwLock<HashMap<String, ProviderBreakerState>>,
    default_config: CircuitBreakerConfig,
}

impl Default for CircuitBreakerRegistry {
    fn default() -> Self {
        Self::new(CircuitBreakerConfig::default())
    }
}

impl CircuitBreakerRegistry {
    pub fn new(default_config: CircuitBreakerConfig) -> Self {
        Self {
            states: RwLock::new(HashMap::new()),
            default_config,
        }
    }

    fn key(app: &str, provider_id: &str) -> String {
        format!("{app}:{provider_id}")
    }

    /// Check if the provider is allowed to receive requests.
    pub fn can_request(&self, app: &str, provider_id: &str) -> bool {
        let key = Self::key(app, provider_id);
        let mut states = match self.states.write() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        let breaker = states
            .entry(key)
            .or_insert_with(|| ProviderBreakerState::new(self.default_config.clone()));
        let state = breaker.check_state();
        state != CircuitState::Open
    }

    /// Record a successful request.
    pub fn record_success(&self, app: &str, provider_id: &str) {
        let key = Self::key(app, provider_id);
        let mut states = match self.states.write() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(breaker) = states.get_mut(&key) {
            breaker.record_success();
        }
    }

    /// Record a failed request (e.g. 429, 5xx, timeout, connection refused).
    pub fn record_failure(&self, app: &str, provider_id: &str) {
        let key = Self::key(app, provider_id);
        let mut states = match self.states.write() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        let breaker = states
            .entry(key)
            .or_insert_with(|| ProviderBreakerState::new(self.default_config.clone()));
        breaker.record_failure();
    }

    /// Reset breaker state for a specific provider.
    pub fn reset(&self, app: &str, provider_id: &str) {
        let key = Self::key(app, provider_id);
        let mut states = match self.states.write() {
            Ok(s) => s,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(breaker) = states.get_mut(&key) {
            breaker.reset();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circuit_breaker_lifecycle() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            cooling_duration: Duration::from_millis(50),
            half_open_success_threshold: 1,
        };
        let registry = CircuitBreakerRegistry::new(config);

        // Initially healthy
        assert!(registry.can_request("claude", "p1"));

        // First failure: still closed
        registry.record_failure("claude", "p1");
        assert!(registry.can_request("claude", "p1"));

        // Second failure: reaches threshold -> Open
        registry.record_failure("claude", "p1");
        assert!(!registry.can_request("claude", "p1"));

        // Wait for cooling duration
        std::thread::sleep(Duration::from_millis(60));

        // Now transitions to HalfOpen, so 1 request allowed
        assert!(registry.can_request("claude", "p1"));

        // Success resets to Closed
        registry.record_success("claude", "p1");
        assert!(registry.can_request("claude", "p1"));
    }
}
