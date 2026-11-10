//! Machine translation of incoming chat: a resident's or an object's
//! nearby line, a resident's message in a conversation, translated by the
//! service the user chose and shown after the original, "text
//! (translation)". Port of LLTranslate and its handlers
//! (indra/newview/lltranslate.cpp) and of translateSuccess in
//! process_chat_from_simulator (llviewermessage.cpp) and LLIMModel::addMessage
//! (llimview.cpp), originally LGPL 2.1.
//!
//! Off by default. The user brings their own key of Azure Translator,
//! Google Cloud Translation or DeepL (Preferences › Chat › Translation and
//! filter), kept in the system's credential store, never in the settings
//! file nor in the log; the text of the messages is never logged either.
//! What is translated is sent to that service: the page says so.
//!
//! Differences, for the user:
//! - a line shows at once and its translation joins it when it comes, where
//!   the Legacy Viewer held the line back until the service answered;
//! - a failure leaves the line as it is (the Legacy Viewer added
//!   "Translation failed: …" to it); the reason goes to the log, without the
//!   text nor the key;
//! - nothing is translated while RLVa hides chat or names (the line would
//!   be read by a third party the restriction does not know of), nor our
//!   own lines, the system's or script errors;
//! - the key is sent in a header, never in the address, and Azure's
//!   endpoint must be one of Microsoft's;
//! - transcripts keep the original text.
//!
//! The requests run on a thread of their own, one at a time, each within
//! `TIMEOUT`; at most `MAX_QUEUED` wait, the rest is not translated.

use super::{ChatLine, Origin, World};
use crate::settings::chat_translation::{AZURE_GLOBAL, ChatTranslation, Service};
use crossbeam_channel::{Receiver, Sender, TrySendError};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

/// How long a request may take.
const TIMEOUT: Duration = Duration::from_secs(10);
/// Requests waiting their turn.
const MAX_QUEUED: usize = 32;
/// Longest text sent (a chat line is 1023 bytes at most).
const MAX_TEXT: usize = 2048;
/// Longest answer read, and longest translation shown.
const MAX_ANSWER: usize = 64 * 1024;
const MAX_TRANSLATION: usize = 4096;
/// The credential store's service of the keys, one account per service.
const STORE: &str = "Aurora Viewer translation";

/// The languages offered (floater_translation_settings.xml), each by its
/// own name, the same in every language of the interface.
pub const LANGUAGES: [(&str, &str); 16] = [
    ("en", "English"),
    ("da", "Dansk"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("hu", "Magyar"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("tr", "Türkçe"),
    ("uk", "Українська"),
    ("zh", "中文 (正體)"),
    ("ja", "日本語"),
    ("ko", "한국어"),
];

/// LLTranslate::getTranslateLanguage: the language chosen, else the
/// interface's, as two letters.
pub fn target(chosen: &str, interface: &str) -> String {
    let chosen = chosen.trim();
    let lang = if LANGUAGES.iter().any(|(code, _)| *code == chosen) {
        chosen
    } else {
        interface
    };
    lang.chars().take(2).collect::<String>().to_ascii_lowercase()
}

/// The service's code of a language. Chinese is Traditional Chinese, as in
/// the Legacy Viewer (getAPILanguageCode); DeepL asks for a variant of
/// English and Portuguese.
fn code(service: Service, lang: &str) -> String {
    match (service, lang) {
        (Service::Azure, "zh") => "zh-Hant".into(),
        (Service::Google, "zh") => "zh-TW".into(),
        (Service::DeepL, "zh") => "ZH-HANT".into(),
        (Service::DeepL, "en") => "EN-US".into(),
        (Service::DeepL, "pt") => "PT-BR".into(),
        (Service::DeepL, other) => other.to_ascii_uppercase(),
        (_, other) => other.to_owned(),
    }
}

/// A key as typed: printable ASCII without spaces (it goes into a header).
pub fn usable_key(key: &str) -> Option<&str> {
    let key = key.trim();
    (!key.is_empty() && key.len() <= 512 && key.bytes().all(|b| b.is_ascii_graphic())).then_some(key)
}

/// Azure's endpoint, when it is one of Microsoft's (the key goes there):
/// the global or a geographical one (*.microsofttranslator.com), or a
/// resource's custom one (*.cognitiveservices.azure.com), over HTTPS.
pub fn azure_endpoint(endpoint: &str) -> Option<String> {
    let endpoint = endpoint.trim();
    let endpoint = if endpoint.is_empty() { AZURE_GLOBAL } else { endpoint };
    let url = reqwest::Url::parse(endpoint).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let microsoft = [".microsofttranslator.com", ".cognitiveservices.azure.com"]
        .iter()
        .any(|suffix| host.ends_with(suffix));
    let plain = url.username().is_empty() && url.password().is_none() && url.query().is_none() && url.fragment().is_none();
    (url.scheme() == "https" && microsoft && plain).then(|| endpoint.trim_end_matches('/').to_owned())
}

/// DeepL's free keys end with ":fx" and have their own server.
fn deepl_base(key: &str) -> &'static str {
    if key.ends_with(":fx") {
        "https://api-free.deepl.com"
    } else {
        "https://api.deepl.com"
    }
}

/// One request to a service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

/// The request translating `text` into `lang` (each handler's
/// getTranslateURL, initHttpHeader and sendMessageAndSuspend, after each
/// service's current reference). None without a usable key or endpoint.
pub fn request(cfg: &ChatTranslation, key: &str, lang: &str, text: &str) -> Option<Request> {
    let key = usable_key(key)?;
    let to = code(cfg.service, lang);
    let json = ("Content-Type", "application/json; charset=UTF-8".to_owned());
    Some(match cfg.service {
        // Translator v3.0: the text as [{"Text": …}], the language detected
        Service::Azure => {
            let mut headers = vec![("Ocp-Apim-Subscription-Key", key.to_owned()), json];
            let region = cfg.azure_region.trim();
            if !region.is_empty() {
                if !region.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return None;
                }
                headers.push(("Ocp-Apim-Subscription-Region", region.to_owned()));
            }
            Request {
                url: format!("{}/translate?api-version=3.0&to={to}", azure_endpoint(&cfg.azure_endpoint)?),
                headers,
                body: serde_json::json!([{ "Text": text }]).to_string(),
            }
        }
        // Cloud Translation Basic (v2), plain text
        Service::Google => Request {
            url: "https://translation.googleapis.com/language/translate/v2".into(),
            headers: vec![("X-goog-api-key", key.to_owned()), json],
            body: serde_json::json!({ "q": text, "target": to, "format": "text" }).to_string(),
        },
        Service::DeepL => Request {
            url: format!("{}/v2/translate", deepl_base(key)),
            headers: vec![("Authorization", format!("DeepL-Auth-Key {key}")), json],
            body: serde_json::json!({ "text": [text], "target_lang": to }).to_string(),
        },
    })
}

/// The translation and the language detected in a service's answer (each
/// handler's parseResponse).
pub fn parse(service: Service, body: &[u8]) -> Option<(String, Option<String>)> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let text = |p: &str| v.pointer(p).and_then(serde_json::Value::as_str).map(str::to_owned);
    let (translation, detected) = match service {
        Service::Azure => (text("/0/translations/0/text")?, text("/0/detectedLanguage/language")),
        Service::Google => (
            text("/data/translations/0/translatedText")?,
            text("/data/translations/0/detectedSourceLanguage"),
        ),
        Service::DeepL => (text("/translations/0/text")?, text("/translations/0/detected_source_language")),
    };
    Some((translation, detected))
}

/// translateSuccess: a translation worth showing, not empty, not of a text
/// already in the language asked for, not the same text again. Kept on one
/// line and bounded.
pub fn accept(original: &str, translation: &str, detected: Option<&str>, lang: &str) -> Option<String> {
    let one_line: String = translation
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_TRANSLATION)
        .collect();
    let one_line = one_line.trim();
    let same_language = detected.is_some_and(|d| d.get(..2).is_some_and(|d| d.eq_ignore_ascii_case(lang)));
    let same_text = one_line.to_lowercase() == original.trim().to_lowercase();
    (!one_line.is_empty() && !same_language && !same_text).then(|| one_line.to_owned())
}

/// What of a line is sent: an emote without its "/me" (CHAT_STYLE_IRC);
/// nothing when it is empty or too long.
pub fn source_text(text: &str) -> Option<&str> {
    let body = match text.get(..4) {
        Some(p) if p.eq_ignore_ascii_case("/me ") || p.eq_ignore_ascii_case("/me'") => &text[3..],
        _ => text,
    };
    let body = body.trim();
    (!body.is_empty() && body.len() <= MAX_TEXT).then_some(body)
}

/// The original line followed by its translation, as translateSuccess
/// writes it.
pub fn with_translation(original: &str, translation: &str) -> String {
    format!("{original} ({translation})")
}

/// RLVa restrictions under which nothing is translated: those that hide
/// or filter received chat, emotes and messages, and those that hide names
/// or places (any option).
const RLVA_HIDING: [&str; 10] = [
    "recvchat",
    "recvchatfrom",
    "recvemote",
    "recvemotefrom",
    "recvim",
    "recvimfrom",
    "shownames",
    "shownametags",
    "shownearby",
    "showloc",
];

pub fn rlva_blocks(rlva: &super::rlva::Rlva) -> bool {
    rlva.enabled() && RLVA_HIDING.iter().any(|b| rlva.has(b) || rlva.has_options(b))
}

/// Where a line is: the nearby chat, or a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Nearby,
    Session(Uuid),
}

/// A line waiting for its translation, found again by its time and sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRef {
    pub place: Place,
    pub time: SystemTime,
    pub source: Uuid,
}

/// Whether a service has a key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeyState {
    /// Not looked at yet.
    #[default]
    Unknown,
    None,
    /// In the credential store.
    Stored,
    /// The store is unavailable: kept in memory until the viewer exits.
    Session,
}

/// The credential store's entry of a service's key (service, account),
/// for the removal from this computer (`uninstall.rs`).
pub fn store_entry(service: Service) -> (&'static str, &'static str) {
    let account = match service {
        Service::Azure => "azure",
        Service::Google => "google",
        Service::DeepL => "deepl",
    };
    (STORE, account)
}

fn entry(service: Service) -> Option<keyring::Entry> {
    if cfg!(test) || aurora_updater::test_environment().is_some() {
        return None;
    }
    let (store, account) = store_entry(service);
    keyring::Entry::new(store, account)
        .map_err(|e| log::warn!("translation: credential store unavailable: {e}"))
        .ok()
}

fn load_key(service: Service) -> Option<String> {
    match entry(service)?.get_password() {
        Ok(key) => usable_key(&key).map(str::to_owned),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            log::warn!("translation: {} key unreadable: {e}", service.name());
            None
        }
    }
}

/// Stores or forgets a key: true when the store holds the change.
fn save_key(service: Service, key: Option<&str>) -> bool {
    let Some(entry) = entry(service) else {
        return false;
    };
    let result = match key {
        Some(key) => entry.set_password(key),
        None => match entry.delete_credential() {
            Err(keyring::Error::NoEntry) => Ok(()),
            other => other,
        },
    };
    result
        .map_err(|e| log::warn!("translation: {} key not kept: {e}", service.name()))
        .is_ok()
}

enum Job {
    Translate {
        line: LineRef,
        cfg: ChatTranslation,
        lang: String,
        text: String,
    },
    Key(Service, Option<String>),
    Probe,
}

enum Reply {
    Translated { line: LineRef, text: String },
    Key(Service, KeyState),
}

/// The translations of the session: the settings, the thread asking the
/// services (started at the first need), the keys' states.
#[derive(Default)]
pub struct Translator {
    cfg: ChatTranslation,
    jobs: Option<Sender<Job>>,
    replies: Option<Receiver<Reply>>,
    keys: HashMap<Service, KeyState>,
    probed: bool,
}

impl Translator {
    pub fn configure(&mut self, cfg: &ChatTranslation) {
        if self.cfg != *cfg {
            self.cfg.clone_from(cfg);
        }
    }

    pub fn on(&self) -> bool {
        self.cfg.on
    }

    pub fn key_state(&self, service: Service) -> KeyState {
        self.keys.get(&service).copied().unwrap_or_default()
    }

    fn sender(&mut self) -> Option<&Sender<Job>> {
        if self.jobs.is_none() {
            let (jobs, rx) = crossbeam_channel::bounded(MAX_QUEUED);
            let (tx, replies) = crossbeam_channel::unbounded();
            let spawned = std::thread::Builder::new()
                .name("translation".into())
                .spawn(move || worker(&rx, &tx));
            if let Err(e) = spawned {
                log::warn!("translation: no thread: {e}");
                return None;
            }
            self.jobs = Some(jobs);
            self.replies = Some(replies);
        }
        self.jobs.as_ref()
    }

    fn send(&mut self, job: Job) {
        if let Some(jobs) = self.sender()
            && let Err(TrySendError::Full(_)) = jobs.try_send(job)
        {
            log::debug!("translation: too many waiting, one left out");
        }
    }

    /// Which services have a key (once, for Preferences).
    pub fn probe(&mut self) {
        if !std::mem::replace(&mut self.probed, true) {
            self.send(Job::Probe);
        }
    }

    /// A key typed in Preferences, or None to forget it.
    pub fn set_key(&mut self, service: Service, key: Option<String>) {
        let key = match key {
            Some(k) => match usable_key(&k) {
                Some(k) => Some(k.to_owned()),
                None => return,
            },
            None => None,
        };
        self.keys.insert(service, KeyState::Unknown);
        self.send(Job::Key(service, key));
    }

    fn translate(&mut self, line: LineRef, text: &str) {
        let Some(text) = source_text(text) else {
            return;
        };
        let lang = target(&self.cfg.language, aurora_i18n::language());
        let job = Job::Translate {
            line,
            cfg: self.cfg.clone(),
            lang,
            text: text.to_owned(),
        };
        self.send(job);
    }

    /// The answers in: translations to show.
    fn poll(&mut self) -> Vec<(LineRef, String)> {
        let mut done = Vec::new();
        let Some(replies) = &self.replies else {
            return done;
        };
        while let Ok(reply) = replies.try_recv() {
            match reply {
                Reply::Translated { line, text } => done.push((line, text)),
                Reply::Key(service, state) => {
                    self.keys.insert(service, state);
                }
            }
        }
        done
    }
}

/// The thread: one job at a time, keys read from the store when first
/// needed.
fn worker(jobs: &Receiver<Job>, replies: &Sender<Reply>) {
    let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => {
            log::warn!("translation: no runtime: {e}");
            return;
        }
    };
    let client = reqwest::Client::builder()
        .user_agent(aurora_version::user_agent())
        .connect_timeout(Duration::from_secs(5))
        .timeout(TIMEOUT)
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build();
    let client = match client {
        Ok(client) => client,
        Err(e) => {
            log::warn!("translation: no HTTP client: {e}");
            return;
        }
    };
    let mut keys: HashMap<Service, (Option<String>, KeyState)> = HashMap::new();
    for job in jobs.iter() {
        match job {
            Job::Probe => {
                for service in Service::ALL {
                    let (_, state) = known(&mut keys, service);
                    let _ = replies.send(Reply::Key(service, state));
                }
            }
            Job::Key(service, key) => {
                let kept = save_key(service, key.as_deref());
                let state = match (&key, kept) {
                    (None, _) => KeyState::None,
                    (Some(_), true) => KeyState::Stored,
                    (Some(_), false) => KeyState::Session,
                };
                keys.insert(service, (key, state));
                let _ = replies.send(Reply::Key(service, state));
            }
            Job::Translate { line, cfg, lang, text } => {
                let (Some(key), _) = known(&mut keys, cfg.service) else {
                    continue;
                };
                let Some(req) = request(&cfg, &key, &lang, &text) else {
                    log::info!("translation: {} not set up", cfg.service.name());
                    continue;
                };
                let answer = runtime.block_on(post(&client, req));
                let translated = answer
                    .map_err(|why| log::info!("translation: {} failed: {why}", cfg.service.name()))
                    .ok()
                    .and_then(|body| parse(cfg.service, &body))
                    .and_then(|(t, detected)| accept(&text, &t, detected.as_deref(), &lang));
                if let Some(text) = translated {
                    let _ = replies.send(Reply::Translated { line, text });
                }
            }
        }
    }
}

/// A service's key, read from the store the first time.
fn known(keys: &mut HashMap<Service, (Option<String>, KeyState)>, service: Service) -> (Option<String>, KeyState) {
    keys.entry(service)
        .or_insert_with(|| match load_key(service) {
            Some(key) => (Some(key), KeyState::Stored),
            None => (None, KeyState::None),
        })
        .clone()
}

/// Sends a request; the reason of a failure names neither the key nor
/// the text.
async fn post(client: &reqwest::Client, req: Request) -> Result<Vec<u8>, String> {
    let mut builder = client.post(&req.url).body(req.body);
    for (name, value) in req.headers {
        builder = builder.header(name, value);
    }
    let mut resp = builder.send().await.map_err(|e| e.without_url().to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.without_url().to_string())? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_ANSWER {
            return Err("answer too large".into());
        }
    }
    Ok(body)
}

impl World {
    /// Translation is wanted now: on, and no RLVa restriction hides chat or
    /// names.
    fn translating(&self) -> bool {
        self.translation.on() && !rlva_blocks(&self.rlva)
    }

    /// A nearby line just added: a resident's or an object's, not ours, not
    /// a script error.
    pub(super) fn translate_chat(&mut self, line: &ChatLine) {
        use super::ChatKind;
        use aurora_net::ChatType;
        let wanted = match line.kind {
            ChatKind::Local(_) => true,
            ChatKind::Object(t) => t != ChatType::Debug,
            _ => false,
        };
        if wanted && self.translating() {
            let at = LineRef {
                place: Place::Nearby,
                time: line.time,
                source: line.source,
            };
            self.translation.translate(at, &line.text);
        }
    }

    /// A message just added to its conversation (`im_messages`): a
    /// resident's (`resident_line`), never ours, the system's or the
    /// blocked's.
    pub fn translate_message(&mut self, m: &crate::ui_sound::ImMessage) {
        if !self.translating() {
            return;
        }
        let Some(line) = self.resident_line(m) else {
            return;
        };
        let at = LineRef {
            place: Place::Session(m.session),
            time: line.time,
            source: line.source,
        };
        let text = line.text.clone();
        self.translation.translate(at, &text);
    }

    /// Every frame: translations arrived joined to their line, while
    /// translation is still wanted.
    pub fn poll_translations(&mut self) {
        let done = self.translation.poll();
        if done.is_empty() || !self.translating() {
            return;
        }
        for (at, text) in done {
            let (lines, clock) = match at.place {
                Place::Nearby => (&mut self.chat, &mut self.chat_clock),
                Place::Session(id) => match self.social.ims.iter_mut().find(|s| s.other == id) {
                    Some(s) => (&mut s.lines, &mut s.clock),
                    None => continue,
                },
            };
            if let Some(line) = lines
                .iter_mut()
                .rev()
                .find(|l| l.time == at.time && l.source == at.source && l.origin == Origin::Live)
            {
                line.text = with_translation(&line.text, &text);
                clock.renumbered();
            }
        }
    }
}

#[cfg(test)]
mod tests;
