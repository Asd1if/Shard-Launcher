//! Подстройка под систему, которую нужно сделать до старта окна.

/// Вызывается из `main` до создания окна, пока работает единственный поток.
pub fn early_init() {
    #[cfg(target_os = "linux")]
    linux::webkit_workarounds();
}

/// Вызывается при старте приложения, до первого обращения к хранилищу учётных данных.
pub fn init_credential_store() {
    #[cfg(target_os = "linux")]
    linux::credential_store();
}

#[cfg(target_os = "linux")]
mod linux {
    /// WebKitGTK с драйвером NVIDIA (особенно под Wayland) часто рисует пустое окно при
    /// DMA-BUF-рендеринге. Выключаем его только для NVIDIA и только если пользователь сам
    /// не задал переменную.
    pub fn webkit_workarounds() {
        let nvidia = std::path::Path::new("/proc/driver/nvidia").exists();
        if nvidia && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            // SAFETY: вызывается из main до запуска других потоков
            unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
        }
    }

    /// По умолчанию секреты живут в keyutils (кеш на сеанс) и в Secret Service (GNOME Keyring,
    /// KWallet), откуда переживают перезагрузку. Без Secret Service (голый оконный менеджер)
    /// запись бы не удалась вовсе — тогда храним только в keyutils, и после перезагрузки
    /// Microsoft-аккаунт придётся войти заново.
    pub fn credential_store() {
        if !secret_service_available() {
            eprintln!("Secret Service is not available; sign-in will last until reboot");
            keyring::set_default_credential_builder(keyring::keyutils::default_credential_builder());
        }
    }

    /// Поиск несуществующей записи не требует разблокировки хранилища и не показывает окон:
    /// NoEntry значит, что служба отвечает.
    fn secret_service_available() -> bool {
        let Ok(entry) = keyring::secret_service::default_credential_builder().build(None, crate::auth::KEYRING_SERVICE, "shard-probe") else {
            return false;
        };
        matches!(entry.get_password(), Err(keyring::Error::NoEntry) | Ok(_))
    }
}
