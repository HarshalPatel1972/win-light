//! The few texts that live outside the webview (tray menu, command names),
//! in the same languages the interface offers.

use windows::Win32::Globalization::GetUserDefaultLocaleName;

const LANGUAGES: &[&str] = &["en", "es", "fr", "de", "pt", "hi", "zh", "ja"];

/// Turn the language setting ("auto" or a code) into a supported language code.
pub fn resolve_language(setting: &str) -> &'static str {
    let wanted = if setting == "auto" { system_language() } else { setting.to_string() };
    let code = wanted.to_lowercase();
    let code = code.split(['-', '_']).next().unwrap_or("en");
    LANGUAGES.iter().find(|known| **known == code).copied().unwrap_or("en")
}

/// The Windows display language, e.g. "en-US".
fn system_language() -> String {
    let mut buffer = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if len <= 1 {
        return "en".to_string();
    }
    String::from_utf16_lossy(&buffer[..len as usize - 1])
}

/// Position of each language in the tables below.
fn column(language: &str) -> usize {
    LANGUAGES.iter().position(|known| *known == language).unwrap_or(0)
}

/// (key, [en, es, fr, de, pt, hi, zh, ja])
const TEXTS: &[(&str, [&str; 8])] = &[
    ("tray.show", ["Show Launcher", "Mostrar el lanzador", "Afficher le lanceur", "Launcher anzeigen", "Mostrar o lançador", "लॉन्चर दिखाएँ", "显示启动器", "ランチャーを表示"]),
    ("tray.settings", ["Settings…", "Ajustes…", "Paramètres…", "Einstellungen…", "Configurações…", "सेटिंग्स…", "设置…", "設定…"]),
    ("tray.rebuild", ["Rebuild Index", "Reconstruir índice", "Reconstruire l'index", "Index neu aufbauen", "Reconstruir índice", "इंडेक्स फिर से बनाएँ", "重建索引", "インデックスを再構築"]),
    ("tray.exit", ["Exit", "Salir", "Quitter", "Beenden", "Sair", "बाहर निकलें", "退出", "終了"]),
    ("command.lock", ["Lock", "Bloquear", "Verrouiller", "Sperren", "Bloquear", "लॉक करें", "锁定", "ロック"]),
    ("command.sleep", ["Sleep", "Suspender", "Mettre en veille", "Energie sparen", "Suspender", "स्लीप", "睡眠", "スリープ"]),
    ("command.signout", ["Sign out", "Cerrar sesión", "Se déconnecter", "Abmelden", "Sair da conta", "साइन आउट", "注销", "サインアウト"]),
    ("command.restart", ["Restart", "Reiniciar", "Redémarrer", "Neu starten", "Reiniciar", "रीस्टार्ट", "重新启动", "再起動"]),
    ("command.shutdown", ["Shut down", "Apagar", "Arrêter", "Herunterfahren", "Desligar", "शट डाउन", "关机", "シャットダウン"]),
    ("command.emptybin", ["Empty Recycle Bin", "Vaciar la papelera", "Vider la corbeille", "Papierkorb leeren", "Esvaziar a lixeira", "रीसायकल बिन खाली करें", "清空回收站", "ごみ箱を空にする"]),
];

/// The text for `key` in `language`, falling back to English.
pub fn text(key: &str, language: &str) -> &'static str {
    TEXTS
        .iter()
        .find(|(known, _)| *known == key)
        .map(|(_, translations)| translations[column(language)])
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_settings_and_locales_to_supported_languages() {
        assert_eq!(resolve_language("de"), "de");
        assert_eq!(resolve_language("pt-BR"), "pt");
        assert_eq!(resolve_language("zh_CN"), "zh");
        assert_eq!(resolve_language("klingon"), "en");
        assert!(LANGUAGES.contains(&resolve_language("auto")));
    }

    #[test]
    fn every_text_exists_in_every_language() {
        for (key, translations) in TEXTS {
            for (i, translation) in translations.iter().enumerate() {
                assert!(!translation.is_empty(), "{} has no {} text", key, LANGUAGES[i]);
            }
        }
        assert_eq!(text("command.lock", "de"), "Sperren");
        assert_eq!(text("tray.exit", "unknown"), "Exit");
        assert_eq!(text("no.such.key", "en"), "");
    }
}
