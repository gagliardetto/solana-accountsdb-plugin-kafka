// Copyright 2022 Blockdaemon Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::{Arc, Mutex};
use {
    crate::*,
    solana_geyser_plugin_interface::geyser_plugin_interface::Result as PluginResult,
    solana_program::pubkey::Pubkey,
    std::{collections::HashSet, str::FromStr},
};
pub struct Filter {
    program_ignores: HashSet<[u8; 32]>,
    program_allowlist: Allowlist,
}
// Copy for Filter
impl Clone for Filter {
    fn clone(&self) -> Self {
        Self {
            program_ignores: self.program_ignores.clone(),
            program_allowlist: self.program_allowlist.clone(),
        }
    }
}

impl Filter {
    pub fn new(config: &Config) -> Self {
        Self {
            program_ignores: config
                .program_ignores
                .iter()
                .flat_map(|p| Pubkey::from_str(p).ok().map(|p| p.to_bytes()))
                .collect(),
            program_allowlist: Allowlist::new_from_config(config).unwrap(),
        }
    }

    pub fn get_allowlist(&self) -> Allowlist {
        self.program_allowlist.clone()
    }

    pub fn wants_program(&self, program: &[u8]) -> bool {
        // If allowlist is not empty, only allowlist is used.
        if self.program_allowlist.len() > 0 {
            return self.program_allowlist.wants_program(program);
        }
        let key = match <&[u8; 32]>::try_from(program) {
            Ok(key) => key,
            _ => return true,
        };
        !self.program_ignores.contains(key)
    }
}

pub struct Allowlist {
    list: Arc<Mutex<HashSet<[u8; 32]>>>,
    http_url: String,
    http_last_updated: Arc<Mutex<std::time::Instant>>,
    http_update_interval: std::time::Duration,
}

// Copy
impl Clone for Allowlist {
    fn clone(&self) -> Self {
        Self {
            list: self.list.clone(),
            http_url: self.http_url.clone(),
            http_last_updated: self.http_last_updated.clone(),
            http_update_interval: self.http_update_interval,
        }
    }
}

// new() is a constructor for Allowlist
impl Allowlist {
    pub fn len(&self) -> usize {
        let list = self.list.lock().unwrap();
        list.len()
    }
    pub fn new_from_config(config: &Config) -> PluginResult<Self> {
        if !config.program_allowlist_url.is_empty() {
            let mut out = Self::new_from_http(
                &config.program_allowlist_url.clone(),
                std::time::Duration::from_secs(config.program_allowlist_update_interval_sec),
            )
            .unwrap();

            if !config.program_allowlist.is_empty() {
                out.push_vec(config.program_allowlist.clone());
            }

            Ok(out)
        } else if !config.program_allowlist.is_empty() {
            Self::new_from_vec(config.program_allowlist.clone())
        } else {
            Ok(Self {
                list: Arc::new(Mutex::new(HashSet::new())),
                http_last_updated: Arc::new(Mutex::new(std::time::Instant::now())),
                http_url: "".to_string(),
                http_update_interval: std::time::Duration::from_secs(0),
            })
        }
    }

    pub fn new_from_vec(program_allowlist: Vec<String>) -> PluginResult<Self> {
        let program_allowlist = program_allowlist
            .iter()
            .flat_map(|p| Pubkey::from_str(p).ok().map(|p| p.to_bytes()))
            .collect();
        Ok(Self {
            list: Arc::new(Mutex::new(program_allowlist)),
            http_last_updated: Arc::new(Mutex::new(std::time::Instant::now())),
            http_url: "".to_string(),
            http_update_interval: std::time::Duration::from_secs(0),
        })
    }

    fn push_vec(&mut self, program_allowlist: Vec<String>) {
        let mut list = self.list.lock().unwrap();
        for pubkey in program_allowlist {
            let pubkey = Pubkey::from_str(&pubkey).unwrap();
            list.insert(pubkey.to_bytes());
        }
    }

    fn get_from_http(url: &str) -> PluginResult<HashSet<[u8; 32]>> {
        let mut program_allowlist = HashSet::new();

        match ureq::get(url).call() {
            Ok(response) => {
                /* the server returned a 200 OK response */
                let body = response.into_string().unwrap();
                let lines = body.lines();
                for line in lines {
                    let pubkey = Pubkey::from_str(line).unwrap();
                    program_allowlist.insert(pubkey.to_bytes());
                }
            }
            Err(ureq::Error::Status(_code, _response)) => {
                // TODO: log error
            }
            Err(_) => {
                /* some kind of io/transport error */
                // TODO: log error
            }
        }

        Ok(program_allowlist)
    }

    pub fn get_last_updated(&self) -> std::time::Instant {
        let v = self.http_last_updated.lock().unwrap();
        *v
    }

    // update_from_http_non_blocking updates the allowlist from a remote URL
    // without blocking the main thread.
    pub fn update_from_http_non_blocking(&self) {
        let list = self.list.clone();
        let http_last_updated = self.http_last_updated.clone();
        let url = self.http_url.clone();
        std::thread::spawn(move || {
            let program_allowlist = Self::get_from_http(&url).unwrap();

            let mut list = list.lock().unwrap();
            *list = program_allowlist;

            let mut http_last_updated = http_last_updated.lock().unwrap();
            *http_last_updated = std::time::Instant::now();
        });
    }

    pub fn should_update_from_http(&self) -> bool {
        let last_updated = self.get_last_updated();
        let now = std::time::Instant::now();
        now.duration_since(last_updated) > self.http_update_interval
    }

    pub fn update_from_http_if_needed_async(&mut self) {
        if self.should_update_from_http() {
            self.update_from_http_non_blocking();
        }
    }

    pub fn update_from_http(&mut self) -> PluginResult<()> {
        if self.http_url.is_empty() {
            return Ok(());
        }
        let program_allowlist = Self::get_from_http(&self.http_url)?;

        let mut list = self.list.lock().unwrap();
        *list = program_allowlist;

        let mut http_last_updated = self.http_last_updated.lock().unwrap();
        *http_last_updated = std::time::Instant::now();
        Ok(())
    }

    pub fn new_from_http(url: &str, interval: std::time::Duration) -> PluginResult<Self> {
        let mut interval = interval;
        if interval < std::time::Duration::from_secs(1) {
            interval = std::time::Duration::from_secs(1);
        }
        let program_allowlist = Self::get_from_http(url)?;
        Ok(Self {
            list: Arc::new(Mutex::new(program_allowlist)),
            // last updated: now
            http_last_updated: Arc::new(Mutex::new(std::time::Instant::now())),
            http_url: url.to_string(),
            http_update_interval: interval,
        })
    }

    pub fn wants_program(&self, program: &[u8]) -> bool {
        let key = match <&[u8; 32]>::try_from(program) {
            Ok(key) => key,
            _ => return true,
        };
        let list = self.list.lock().unwrap();
        list.is_empty() || list.contains(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter() {
        let config = Config {
            program_ignores: vec![
                "Sysvar1111111111111111111111111111111111111".to_owned(),
                "Vote111111111111111111111111111111111111111".to_owned(),
            ],
            ..Config::default()
        };

        let filter = Filter::new(&config);
        assert_eq!(filter.program_ignores.len(), 2);

        assert!(filter.wants_program(
            &Pubkey::from_str("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")
                .unwrap()
                .to_bytes()
        ));
        assert!(!filter.wants_program(
            &Pubkey::from_str("Vote111111111111111111111111111111111111111")
                .unwrap()
                .to_bytes()
        ));
    }

    #[test]
    fn test_allowlist_from_vec() {
        let config = Config {
            program_allowlist: vec![
                "Sysvar1111111111111111111111111111111111111".to_owned(),
                "Vote111111111111111111111111111111111111111".to_owned(),
            ],
            ..Config::default()
        };

        let allowlist = Allowlist::new_from_vec(config.program_allowlist).unwrap();
        assert_eq!(allowlist.len(), 2);

        assert!(allowlist.wants_program(
            &Pubkey::from_str("Sysvar1111111111111111111111111111111111111")
                .unwrap()
                .to_bytes()
        ));
        assert!(allowlist.wants_program(
            &Pubkey::from_str("Vote111111111111111111111111111111111111111")
                .unwrap()
                .to_bytes()
        ));
        // negative test
        assert!(!allowlist.wants_program(
            &Pubkey::from_str("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")
                .unwrap()
                .to_bytes()
        ));
    }

    #[test]
    fn test_allowlist_from_http() {
        // create fake http server
        let _m = mockito::mock("GET", "/allowlist.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("Sysvar1111111111111111111111111111111111111\nVote111111111111111111111111111111111111111")
            .create();

        let config = Config {
            program_allowlist_url: [mockito::server_url(), "/allowlist.txt".to_owned()].join(""),
            program_allowlist_update_interval_sec: 3,
            program_allowlist: vec!["WormT3McKhFJ2RkiGpdw9GKvNCrB2aB54gb2uV9MfQC".to_owned()],
            ..Config::default()
        };

        let mut allowlist = Allowlist::new_from_config(&config).unwrap();
        assert_eq!(allowlist.len(), 3);
        assert!(!allowlist.should_update_from_http());

        assert!(allowlist.wants_program(
            &Pubkey::from_str("WormT3McKhFJ2RkiGpdw9GKvNCrB2aB54gb2uV9MfQC")
                .unwrap()
                .to_bytes()
        ));
        assert!(allowlist.wants_program(
            &Pubkey::from_str("Sysvar1111111111111111111111111111111111111")
                .unwrap()
                .to_bytes()
        ));
        assert!(allowlist.wants_program(
            &Pubkey::from_str("Vote111111111111111111111111111111111111111")
                .unwrap()
                .to_bytes()
        ));
        // negative test
        assert!(!allowlist.wants_program(
            &Pubkey::from_str("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")
                .unwrap()
                .to_bytes()
        ));

        {
            let _u = mockito::mock("GET", "/allowlist.txt")
                .with_status(200)
                .with_header("content-type", "text/plain")
                .with_body("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")
                .create();
            allowlist.update_from_http().unwrap();
            assert_eq!(allowlist.len(), 1);

            assert!(allowlist.wants_program(
                &Pubkey::from_str("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")
                    .unwrap()
                    .to_bytes()
            ));
        }
        {
            let _u = mockito::mock("GET", "/allowlist.txt")
                .with_status(200)
                .with_header("content-type", "text/plain")
                .with_body("")
                .create();
            let last_updated = allowlist.get_last_updated();
            println!("last_updated: {:?}", last_updated);
            allowlist.update_from_http().unwrap();
            assert_ne!(allowlist.get_last_updated(), last_updated);
            assert_eq!(allowlist.len(), 0);
            println!("last_updated: {:?}", allowlist.get_last_updated());

            assert!(allowlist.wants_program(
                &Pubkey::from_str("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")
                    .unwrap()
                    .to_bytes()
            ));
        }
        {
            // async
            let _u = mockito::mock("GET", "/allowlist.txt")
                .with_status(200)
                .with_header("content-type", "text/plain")
                .with_body("Sysvar1111111111111111111111111111111111111\nVote111111111111111111111111111111111111111")
                .create();

            let last_updated = allowlist.get_last_updated();
            allowlist.update_from_http_non_blocking();
            // the values should be the same because it returns immediately
            // before the async task completes
            assert_eq!(allowlist.get_last_updated(), last_updated);
            assert_eq!(allowlist.len(), 0);
            // sleep for 1 second to allow the async task to complete
            std::thread::sleep(std::time::Duration::from_secs(1));
            assert!(!allowlist.should_update_from_http());

            assert_eq!(allowlist.len(), 2);
            assert_ne!(allowlist.get_last_updated(), last_updated);

            assert!(allowlist.wants_program(
                &Pubkey::from_str("Sysvar1111111111111111111111111111111111111")
                    .unwrap()
                    .to_bytes()
            ));
            assert!(allowlist.wants_program(
                &Pubkey::from_str("Vote111111111111111111111111111111111111111")
                    .unwrap()
                    .to_bytes()
            ));
            // negative test
            assert!(!allowlist.wants_program(
                &Pubkey::from_str("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin")
                    .unwrap()
                    .to_bytes()
            ));

            std::thread::sleep(std::time::Duration::from_secs(3));
            assert!(allowlist.should_update_from_http());
        }
    }
}
