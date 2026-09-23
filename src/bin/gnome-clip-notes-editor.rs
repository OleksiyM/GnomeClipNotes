#[path = "../preview_html.rs"]
mod document;
#[path = "../preview_webkit.rs"]
mod web_preview;

fn main() -> glib::ExitCode {
    gnome_clip_notes::run_editor(std::rc::Rc::new(web_preview::build))
}
