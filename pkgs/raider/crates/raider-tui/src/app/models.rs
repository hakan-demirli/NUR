use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::dialog::DialogOption;
use crate::provider::{ModelCatalog, ModelRef};

pub const RECENT_MODELS_CAP: usize = 10;

pub struct ModelState {
    pub catalog: ModelCatalog,
    pub current_model: Option<ModelRef>,
    pub current_variant: Option<String>,
    pub variant_map: HashMap<String, String>,
    pub recent_models: Vec<ModelRef>,
    pub favorite_models: Vec<ModelRef>,
    pub thinking_hidden: Option<bool>,
}

impl ModelState {
    pub fn new() -> Self {
        Self {
            catalog: ModelCatalog::default(),
            current_model: None,
            current_variant: None,
            variant_map: HashMap::new(),
            recent_models: Vec::new(),
            favorite_models: Vec::new(),
            thinking_hidden: None,
        }
    }

    pub fn set_recent(&mut self, recent: Vec<ModelRef>) {
        self.recent_models = recent;
        self.recent_models.truncate(RECENT_MODELS_CAP);
    }

    pub fn set_favorites(&mut self, favorites: Vec<ModelRef>) {
        self.favorite_models = favorites;
    }

    pub fn set_current_model(&mut self, model: Option<ModelRef>) {
        match model {
            None => {
                self.current_model = None;
                self.current_variant = None;
            }
            Some(incoming) => {
                let keep_existing = self
                    .current_model
                    .as_ref()
                    .map(|cur| self.catalog.is_empty() || self.catalog.has(cur))
                    .unwrap_or(false);
                if keep_existing {
                    return;
                }
                let key = incoming.wire();
                self.current_model = Some(incoming);
                self.current_variant = self.variant_map.get(&key).cloned();
            }
        }
    }

    pub fn set_current_variant(&mut self, variant: Option<String>) {
        if let Some(m) = &self.current_model {
            let key = m.wire();
            match &variant {
                Some(v) => {
                    self.variant_map.insert(key, v.clone());
                }
                None => {
                    self.variant_map.remove(&key);
                }
            }
        }
        self.current_variant = variant;
    }

    pub fn set_catalog(&mut self, catalog: ModelCatalog) {
        self.catalog = catalog;
        if let Some(m) = &self.current_model {
            if !self.catalog.has(m) {
                self.current_model = None;
                self.current_variant = None;
            }
        }
        if self.current_variant.is_none() {
            if let Some(m) = &self.current_model {
                self.current_variant = self.variant_map.get(&m.wire()).cloned();
            }
        }
        if let (Some(m), Some(v)) = (&self.current_model, self.current_variant.clone()) {
            let still_has = self
                .catalog
                .find(m)
                .map(|(_, mi)| mi.variants.iter().any(|x| x == &v))
                .unwrap_or(false);
            if !still_has {
                self.current_variant = None;
            }
        }
    }

    pub fn catalog_wire_refs(&self) -> Vec<String> {
        self.catalog
            .all_refs()
            .into_iter()
            .map(|m| m.wire())
            .collect()
    }

    pub fn current_variant_list(&self) -> Vec<String> {
        let Some(m) = &self.current_model else {
            return Vec::new();
        };
        self.catalog
            .find(m)
            .map(|(_, mi)| mi.variants.clone())
            .unwrap_or_default()
    }

    pub fn touch_recent(&mut self, model: &ModelRef) {
        self.recent_models.retain(|m| m != model);
        self.recent_models.insert(0, model.clone());
        if self.recent_models.len() > RECENT_MODELS_CAP {
            self.recent_models.truncate(RECENT_MODELS_CAP);
        }
    }

    pub fn toggle_favorite(&mut self, model: ModelRef) {
        if let Some(pos) = self.favorite_models.iter().position(|m| m == &model) {
            self.favorite_models.remove(pos);
        } else {
            self.favorite_models.insert(0, model);
        }
    }

    pub fn pick_recent(&mut self, delta: i32) -> Option<ModelRef> {
        if self.recent_models.len() < 2 {
            return None;
        }
        let cur = self.current_model.clone();
        let idx = cur
            .as_ref()
            .and_then(|m| self.recent_models.iter().position(|x| x == m))
            .unwrap_or(0);
        let len = self.recent_models.len() as i32;
        let next = ((idx as i32 + delta).rem_euclid(len)) as usize;
        Some(self.recent_models[next].clone())
    }

    #[allow(clippy::result_unit_err)]
    pub fn pick_favorite(&mut self, delta: i32) -> Result<ModelRef, ()> {
        if self.favorite_models.is_empty() {
            return Err(());
        }
        let cur = self.current_model.clone();
        let idx = cur
            .as_ref()
            .and_then(|m| self.favorite_models.iter().position(|x| x == m))
            .map(|i| i as i32)
            .unwrap_or(if delta >= 0 {
                -1
            } else {
                self.favorite_models.len() as i32
            });
        let len = self.favorite_models.len() as i32;
        let next = ((idx + delta).rem_euclid(len)) as usize;
        Ok(self.favorite_models[next].clone())
    }

    pub fn next_variant(&self) -> Option<Option<String>> {
        let variants = self.current_variant_list();
        if variants.is_empty() {
            return None;
        }
        let next = match &self.current_variant {
            None => Some(variants[0].clone()),
            Some(cur) => match variants.iter().position(|v| v == cur) {
                Some(i) if i + 1 == variants.len() => None,
                Some(i) => Some(variants[i + 1].clone()),
                None => Some(variants[0].clone()),
            },
        };
        Some(next)
    }

    pub fn build_picker_options(&self) -> Vec<DialogOption> {
        let mut options: Vec<DialogOption> = Vec::new();

        let fav_rows: Vec<(ModelRef, String, String)> = self
            .favorite_models
            .iter()
            .filter_map(|m| {
                self.catalog.find(m).map(|(p, mi)| {
                    (
                        m.clone(),
                        mi.display_name().to_string(),
                        p.display_name().to_string(),
                    )
                })
            })
            .collect();
        if !fav_rows.is_empty() {
            options.push(DialogOption::header("Favorites"));
            for (m, title, provider) in &fav_rows {
                let mut opt =
                    DialogOption::new(title.clone(), format!("{}/{}", m.provider_id, m.model_id));
                opt.category = Some(provider.clone());
                options.push(opt);
            }
        }

        let recent_rows: Vec<(ModelRef, String, String)> = self
            .recent_models
            .iter()
            .filter(|m| !self.favorite_models.iter().any(|f| f == *m))
            .filter_map(|m| {
                self.catalog.find(m).map(|(p, mi)| {
                    (
                        m.clone(),
                        mi.display_name().to_string(),
                        p.display_name().to_string(),
                    )
                })
            })
            .collect();
        if !recent_rows.is_empty() {
            options.push(DialogOption::header("Recent"));
            for (m, title, provider) in &recent_rows {
                let mut opt =
                    DialogOption::new(title.clone(), format!("{}/{}", m.provider_id, m.model_id));
                opt.category = Some(provider.clone());
                options.push(opt);
            }
        }

        for p in &self.catalog.providers {
            let rows: Vec<&crate::provider::ModelInfo> = p
                .models
                .iter()
                .filter(|m| {
                    let in_fav = self
                        .favorite_models
                        .iter()
                        .any(|f| f.provider_id == p.id && f.model_id == m.id);
                    let in_recent = self
                        .recent_models
                        .iter()
                        .any(|r| r.provider_id == p.id && r.model_id == m.id);
                    !in_fav && !in_recent
                })
                .collect();
            if rows.is_empty() {
                continue;
            }
            options.push(DialogOption::header(p.display_name().to_string()));
            for m in rows {
                let mut opt =
                    DialogOption::new(m.display_name().to_string(), format!("{}/{}", p.id, m.id));
                opt.category = Some(p.display_name().to_string());
                options.push(opt);
            }
        }
        options
    }

    pub fn save_to_disk(&self) -> std::io::Result<()> {
        let Some(path) = model_state_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = PersistedModelState {
            current: self.current_model.as_ref().map(Into::into),
            recent: self.recent_models.iter().map(Into::into).collect(),
            favorite: self.favorite_models.iter().map(Into::into).collect(),
            thinking_hidden: self.thinking_hidden,
            variant: self.variant_map.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&payload).map_err(std::io::Error::other)?;
        std::fs::write(&path, bytes)
    }

    pub fn load_from_disk(&mut self) {
        let Some(path) = model_state_path() else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let Ok(state) = serde_json::from_slice::<PersistedModelState>(&bytes) else {
            return;
        };
        if let Some(c) = state.current {
            self.current_model = Some(c.into());
        }
        self.recent_models = state.recent.into_iter().map(Into::into).collect();
        self.recent_models.truncate(RECENT_MODELS_CAP);
        self.favorite_models = state.favorite.into_iter().map(Into::into).collect();
        self.thinking_hidden = state.thinking_hidden;
        self.variant_map = state.variant;
        if let Some(m) = &self.current_model {
            self.current_variant = self.variant_map.get(&m.wire()).cloned();
        }
    }
}

impl Default for ModelState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct PersistedModelState {
    #[serde(default)]
    current: Option<PersistedModelRef>,
    #[serde(default)]
    recent: Vec<PersistedModelRef>,
    #[serde(default)]
    favorite: Vec<PersistedModelRef>,
    #[serde(default, rename = "thinkingHidden")]
    thinking_hidden: Option<bool>,
    #[serde(default)]
    variant: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedModelRef {
    #[serde(rename = "providerID")]
    provider_id: String,
    #[serde(rename = "modelID")]
    model_id: String,
}

impl From<&ModelRef> for PersistedModelRef {
    fn from(m: &ModelRef) -> Self {
        Self {
            provider_id: m.provider_id.clone(),
            model_id: m.model_id.clone(),
        }
    }
}

impl From<PersistedModelRef> for ModelRef {
    fn from(p: PersistedModelRef) -> Self {
        ModelRef {
            provider_id: p.provider_id,
            model_id: p.model_id,
        }
    }
}

fn model_state_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RAIDER_STATE_DIR") {
        let s = dir.to_string_lossy();
        if s.is_empty() {
            return None;
        }
        return Some(PathBuf::from(dir).join("model.json"));
    }
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        return Some(PathBuf::from(xdg).join("raider").join("model.json"));
    }
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("raider")
            .join("model.json"),
    )
}
