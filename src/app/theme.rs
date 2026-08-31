use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub struct ThemeController {
    dark: Signal<bool>,
}

impl ThemeController {
    pub fn is_dark(self) -> bool {
        *self.dark.read()
    }

    pub fn toggle(mut self) {
        let dark = !self.is_dark();
        self.dark.set(dark);

        #[cfg(feature = "web")]
        document::eval(if dark {
            r#"localStorage.setItem("lensy-theme", "dark");"#
        } else {
            r#"localStorage.setItem("lensy-theme", "light");"#
        });
    }
}

pub fn use_theme_provider() -> ThemeController {
    let controller = ThemeController {
        dark: use_signal(|| false),
    };

    use_context_provider(|| controller);

    use_effect(move || {
        #[cfg(feature = "web")]
        spawn(async move {
            let restored =
                document::eval(r#"return localStorage.getItem("lensy-theme") === "dark";"#)
                    .join::<bool>()
                    .await
                    .unwrap_or(false);

            let mut dark = controller.dark;
            dark.set(restored);
        });
    });

    controller
}

pub fn use_theme() -> ThemeController {
    use_context::<ThemeController>()
}
