fn main() {
    topcoat::tailwind::BuildConfig::new()
        .input("assets/styles.css")
        .render()
        .expect("compile Tailwind styles");

    topcoat::icon::iconify::BuildConfig::new()
        .icon_set("lucide")
        .stage()
        .expect("stage Lucide icons");
}
