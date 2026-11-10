//! The translation of incoming chat (Preferences › Chat › Translation and
//! filter): the Legacy Viewer's TranslateChat, TranslateLanguage and
//! TranslationService (indra/newview/app_settings/settings.xml) and the
//! non-secret part of its AzureTranslateAPIKey and DeepLTranslateAPIKey.
//! The keys themselves are never here: they are kept in the system's
//! credential store (`world/translate.rs`), where the Legacy Viewer wrote
//! them to its settings file.

use serde::{Deserialize, Serialize};

/// A translation service (LLTranslate::EService).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Service {
    /// The Legacy default.
    #[default]
    Azure,
    Google,
    DeepL,
}

impl Service {
    pub const ALL: [Service; 3] = [Service::Azure, Service::Google, Service::DeepL];

    /// Its name, as its brand writes it.
    pub fn name(self) -> &'static str {
        match self {
            Service::Azure => "Azure Translator",
            Service::Google => "Google Translate",
            Service::DeepL => "DeepL",
        }
    }
}

/// Azure's global endpoint, the Legacy default of azure_api_endpoint_combo.
pub const AZURE_GLOBAL: &str = "https://api.cognitive.microsofttranslator.com";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatTranslation {
    /// TranslateChat (off): nearby chat and conversations translated as
    /// they arrive. Chat text is sent to the service chosen.
    pub on: bool,
    /// TranslationService.
    pub service: Service,
    /// TranslateLanguage: a two-letter code, empty for the viewer's own
    /// language ("default").
    pub language: String,
    /// The endpoint of the Azure resource (AzureTranslateAPIKey "endpoint").
    pub azure_endpoint: String,
    /// Its region, for a regional or multi-service resource
    /// (AzureTranslateAPIKey "region"); empty for a global one.
    pub azure_region: String,
}

impl Default for ChatTranslation {
    fn default() -> Self {
        ChatTranslation {
            on: false,
            service: Service::Azure,
            language: String::new(),
            azure_endpoint: AZURE_GLOBAL.to_owned(),
            azure_region: String::new(),
        }
    }
}
