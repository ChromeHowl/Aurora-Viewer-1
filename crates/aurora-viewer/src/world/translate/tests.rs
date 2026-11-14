use super::*;
use crate::world::rlva::Rlva;
use aurora_net::{ChatMessage, ChatSourceType, ChatType};

const KEY: &str = "test-key-0123";

fn cfg(service: Service) -> ChatTranslation {
    ChatTranslation {
        on: true,
        service,
        ..ChatTranslation::default()
    }
}

fn header<'a>(r: &'a Request, name: &str) -> Option<&'a str> {
    r.headers.iter().find(|(n, _)| *n == name).map(|(_, v)| v.as_str())
}

#[test]
fn off_by_default() {
    let d = ChatTranslation::default();
    assert!(!d.on);
    assert_eq!(d.service, Service::Azure, "the Legacy default service");
    assert_eq!(d.azure_endpoint, AZURE_GLOBAL);
}

#[test]
fn the_language_is_the_chosen_one_or_the_interface_s() {
    assert_eq!(target("", "fr"), "fr");
    assert_eq!(target("de", "fr"), "de");
    assert_eq!(target("xx", "ja"), "ja", "an unknown choice falls back");
    assert_eq!(target("", "pt-BR"), "pt");
}

#[test]
fn azure_request() {
    let mut c = cfg(Service::Azure);
    c.azure_region = "westeurope".into();
    let r = request(&c, KEY, "zh", "Hello").expect("request");
    assert_eq!(
        r.url,
        "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=zh-Hant"
    );
    assert_eq!(header(&r, "Ocp-Apim-Subscription-Key"), Some(KEY));
    assert_eq!(header(&r, "Ocp-Apim-Subscription-Region"), Some("westeurope"));
    let body: serde_json::Value = serde_json::from_str(&r.body).expect("json");
    assert_eq!(body, serde_json::json!([{ "Text": "Hello" }]));
    // a global resource sends no region
    let r = request(&cfg(Service::Azure), KEY, "fr", "Hello").expect("request");
    assert_eq!(header(&r, "Ocp-Apim-Subscription-Region"), None);
    // a region that would break the header
    c.azure_region = "west\r\neurope".into();
    assert_eq!(request(&c, KEY, "fr", "Hello"), None);
}

#[test]
fn azure_endpoints_must_be_microsoft_s() {
    assert_eq!(azure_endpoint(""), Some(AZURE_GLOBAL.to_owned()));
    assert_eq!(
        azure_endpoint("https://api-eur.cognitive.microsofttranslator.com/"),
        Some("https://api-eur.cognitive.microsofttranslator.com".to_owned())
    );
    assert!(azure_endpoint("https://my-res.cognitiveservices.azure.com").is_some());
    assert_eq!(azure_endpoint("http://api.cognitive.microsofttranslator.com"), None, "plain HTTP");
    assert_eq!(azure_endpoint("https://evil.example/microsofttranslator.com"), None);
    assert_eq!(azure_endpoint("https://microsofttranslator.com.evil.example"), None);
    assert_eq!(azure_endpoint("https://user@api.cognitive.microsofttranslator.com"), None);
    let mut c = cfg(Service::Azure);
    c.azure_endpoint = "https://collector.example".into();
    assert_eq!(request(&c, KEY, "fr", "Hello"), None, "the key never leaves for another host");
}

#[test]
fn google_request_keeps_the_key_out_of_the_address() {
    let r = request(&cfg(Service::Google), KEY, "ja", "Hello & bye").expect("request");
    assert_eq!(r.url, "https://translation.googleapis.com/language/translate/v2");
    assert!(!r.url.contains(KEY));
    assert_eq!(header(&r, "X-goog-api-key"), Some(KEY));
    let body: serde_json::Value = serde_json::from_str(&r.body).expect("json");
    assert_eq!(body, serde_json::json!({ "q": "Hello & bye", "target": "ja", "format": "text" }));
}

#[test]
fn deepl_request_picks_the_free_server_for_a_free_key() {
    let r = request(&cfg(Service::DeepL), "abc:fx", "en", "Bonjour").expect("request");
    assert_eq!(r.url, "https://api-free.deepl.com/v2/translate");
    assert_eq!(header(&r, "Authorization"), Some("DeepL-Auth-Key abc:fx"));
    let body: serde_json::Value = serde_json::from_str(&r.body).expect("json");
    assert_eq!(body, serde_json::json!({ "text": ["Bonjour"], "target_lang": "EN-US" }));
    let r = request(&cfg(Service::DeepL), "abc", "de", "Hi").expect("request");
    assert_eq!(r.url, "https://api.deepl.com/v2/translate");
    assert!(r.body.contains("\"DE\""));
}

#[test]
fn unusable_keys_send_nothing() {
    for key in ["", "   ", "two words", "line\nbreak", "é"] {
        assert_eq!(request(&cfg(Service::Google), key, "fr", "x"), None, "{key:?}");
    }
    assert_eq!(usable_key("  abc  "), Some("abc"));
    assert_eq!(usable_key(&"k".repeat(513)), None);
}

#[test]
fn answers_of_each_service() {
    let azure = br#"[{"detectedLanguage":{"language":"en","score":1.0},"translations":[{"text":"Bonjour","to":"fr"}]}]"#;
    assert_eq!(parse(Service::Azure, azure), Some(("Bonjour".into(), Some("en".into()))));
    let google = br#"{"data":{"translations":[{"translatedText":"Hallo","detectedSourceLanguage":"en"}]}}"#;
    assert_eq!(parse(Service::Google, google), Some(("Hallo".into(), Some("en".into()))));
    let deepl = br#"{"translations":[{"detected_source_language":"FR","text":"Hello"}]}"#;
    assert_eq!(parse(Service::DeepL, deepl), Some(("Hello".into(), Some("FR".into()))));
    // errors and garbage give nothing
    assert_eq!(parse(Service::Azure, br#"{"error":{"code":401000,"message":"denied"}}"#), None);
    assert_eq!(parse(Service::Google, br#"{"error":{"code":400}}"#), None);
    assert_eq!(parse(Service::DeepL, b"<html>"), None);
}

#[test]
fn only_useful_translations_are_shown() {
    assert_eq!(accept("Bonjour", "Hello", Some("fr"), "en"), Some("Hello".into()));
    assert_eq!(accept("Hello", "Hello", Some("en"), "en"), None, "already in the language");
    assert_eq!(accept("Hello", "Hello", Some("EN"), "en"), None, "DeepL's upper case");
    assert_eq!(accept("lol", "LOL", None, "en"), None, "the same text");
    assert_eq!(accept("x", "   ", Some("fr"), "en"), None, "empty");
    assert_eq!(accept("a", "two\nlines", Some("fr"), "en"), Some("two lines".into()));
    let long = "y".repeat(MAX_TRANSLATION + 100);
    assert_eq!(
        accept("a", &long, Some("fr"), "en").map(|t| t.chars().count()),
        Some(MAX_TRANSLATION)
    );
}

#[test]
fn what_of_a_line_is_sent() {
    assert_eq!(source_text("/me waves hello"), Some("waves hello"));
    assert_eq!(source_text("/me's hat"), Some("'s hat"));
    assert_eq!(source_text("  hi  "), Some("hi"));
    assert_eq!(source_text("   "), None);
    assert_eq!(source_text(&"z".repeat(MAX_TEXT + 1)), None);
    assert_eq!(with_translation("Bonjour", "Hello"), "Bonjour (Hello)");
}

fn rlva_with(command: &str) -> Rlva {
    let mut r = Rlva::default();
    r.configure(true);
    let c = ChatMessage {
        from_name: "RLVa test object".to_owned(),
        source_id: Uuid::from_u128(1),
        owner_id: Uuid::from_u128(2),
        source_type: ChatSourceType::Object,
        chat_type: ChatType::OwnerSay,
        position: glam::Vec3::ZERO,
        message: command.to_owned(),
        region: None,
    };
    assert!(r.receive(&c));
    r
}

#[test]
fn rlva_restrictions_on_chat_or_names_stop_translation() {
    assert!(!rlva_blocks(&Rlva::default()));
    assert!(!rlva_blocks(&rlva_with("@fly=n")));
    for command in [
        "@recvchat=n",
        "@recvemote=n",
        "@recvim=n",
        "@shownames=n",
        "@shownametags=n",
        "@showloc=n",
        "@recvchatfrom:00000000-0000-0000-0000-000000000009=n",
    ] {
        assert!(rlva_blocks(&rlva_with(command)), "{command}");
    }
}

#[test]
fn keys_go_to_their_own_store_entries() {
    let entries: Vec<_> = Service::ALL.iter().map(|s| store_entry(*s)).collect();
    assert_eq!(
        entries,
        [
            ("Aurora Viewer translation", "azure"),
            ("Aurora Viewer translation", "google"),
            ("Aurora Viewer translation", "deepl")
        ]
    );
}

#[test]
fn deepl_codes_are_those_its_reference_names() {
    let target = |lang: &str| {
        let r = request(&cfg(Service::DeepL), KEY, lang, "Hello").expect("request");
        let body: serde_json::Value = serde_json::from_str(&r.body).expect("json");
        body["target_lang"].as_str().unwrap_or_default().to_owned()
    };
    assert_eq!(target("pt"), "PT-BR");
    assert_eq!(target("en"), "EN-US");
    assert_eq!(target("zh"), "ZH", "no Traditional Chinese variant named: the base code");
    assert_eq!(target("ja"), "JA");
}

#[test]
fn azure_leaves_the_links_as_they_are() {
    let text = "see https://example.com/a?b=1 and secondlife:///app/agent/x/about now";
    let sent = protect_links(Service::Azure, text);
    assert_eq!(
        sent,
        "see <div translate=\"no\">https://example.com/a?b=1</div> and <div translate=\"no\">secondlife:///app/agent/x/about</div> now"
    );
    assert_eq!(protect_links(Service::DeepL, text), text);
    assert_eq!(protect_links(Service::Google, text), text);
    let back = "voir <div translate=\"no\">https://example.com/a?b=1</div> et <div translate=\"no\">www.x.org</div>";
    assert_eq!(unprotect_links(Service::Azure, back), "voir https://example.com/a?b=1 et www.x.org");
    assert_eq!(unprotect_links(Service::DeepL, back), back);
}
