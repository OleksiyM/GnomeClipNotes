//! Noninteractive metadata tags: fit text inside the space the card gives them,
//! without letting a long name dictate the card/grid's preferred width.
use gtk::{prelude::*, subclass::prelude::*};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct MetadataTag;

    #[glib::object_subclass]
    impl ObjectSubclass for MetadataTag {
        const NAME: &'static str = "GcnMetadataTag";
        type Type = super::MetadataTag;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for MetadataTag {
        fn dispose(&self) {
            if let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for MetadataTag {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let Some(child) = self.obj().first_child() else {
                return (0, 0, -1, -1);
            };
            let measured = child.measure(orientation, for_size);
            if orientation == gtk::Orientation::Horizontal {
                // The body determines card width. Reserve only the tag's minimum;
                // its full natural width is used later, during allocation.
                (measured.0, measured.0, -1, -1)
            } else {
                measured
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let obj = self.obj();
            if let Some(child) = obj.first_child() {
                let tag_width = child
                    .measure(gtk::Orientation::Horizontal, height)
                    .1
                    .min(width);
                let x = if obj.direction() == gtk::TextDirection::Rtl {
                    width - tag_width
                } else {
                    0
                };
                child.size_allocate(&gtk::Allocation::new(x, 0, tag_width, height), baseline);
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            if let Some(child) = self.obj().first_child() {
                self.obj().snapshot_child(&child, snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct MetadataTag(ObjectSubclass<imp::MetadataTag>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl MetadataTag {
    pub fn new(icon: &str, text: &str, name: &str) -> Self {
        let tag: Self = glib::Object::builder().build();
        tag.set_widget_name(name);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.add_css_class("metadata-tag");
        row.set_tooltip_text(Some(text));
        row.append(&gtk::Image::from_icon_name(icon));
        row.append(
            &gtk::Label::builder()
                .label(text)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build(),
        );
        row.set_parent(&tag);
        tag
    }
}
