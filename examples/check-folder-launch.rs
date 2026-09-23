//! Manual launch probe: creates its own empty temporary directory, no app database.
use adw::prelude::*;

fn main() {
    adw::init().unwrap();
    let path = std::env::temp_dir().join(format!("gcn-folder-launch-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    let file = gio::File::for_path(&path);
    let window = adw::Window::builder()
        .title("ClipNotes folder launch check")
        .default_width(360)
        .default_height(120)
        .build();
    window.set_content(Some(&gtk::Label::new(Some(
        "Checking the system file manager…",
    ))));
    window.present();
    let context = glib::MainContext::default();
    context.block_on(async {
        glib::timeout_future(std::time::Duration::from_millis(300)).await;
        let result = if std::env::args().any(|arg| arg == "--uri") {
            gtk::UriLauncher::new(file.uri().as_str())
                .launch_future(Some(&window))
                .await
        } else {
            gtk::FileLauncher::new(Some(&file))
                .launch_future(Some(&window))
                .await
        };
        window.close();
        match result {
            Ok(()) => println!("PASS: file manager opened {}", path.display()),
            Err(error) => {
                eprintln!("FAIL: {error:?}");
                std::process::exit(1);
            }
        }
    });
}
