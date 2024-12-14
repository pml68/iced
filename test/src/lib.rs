//! Test your `iced` applications in headless mode.
#![allow(missing_docs, missing_debug_implementations)]
pub mod selector;

pub use selector::Selector;

use iced_renderer as renderer;
use iced_runtime as runtime;
use iced_runtime::core;

use crate::core::clipboard;
use crate::core::keyboard;
use crate::core::mouse;
use crate::core::theme;
use crate::core::time;
use crate::core::widget;
use crate::core::window;
use crate::core::{Element, Event, Font, Rectangle, Settings, Size, SmolStr};
use crate::runtime::user_interface;
use crate::runtime::UserInterface;

use std::borrow::Cow;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Arc;

pub fn simulator<'a, Message, Theme, Renderer>(
    element: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Simulator<'a, Message, Theme, Renderer>
where
    Theme: Default + theme::Base,
    Renderer: core::Renderer + core::renderer::Headless,
{
    Simulator::new(element)
}

fn load_font(font: impl Into<Cow<'static, [u8]>>) -> Result<(), Error> {
    renderer::graphics::text::font_system()
        .write()
        .expect("Write to font system")
        .load_font(font.into());

    Ok(())
}

pub struct Simulator<
    'a,
    Message,
    Theme = core::Theme,
    Renderer = renderer::Renderer,
> {
    raw: UserInterface<'a, Message, Theme, Renderer>,
    renderer: Renderer,
    window_size: Size,
    messages: Vec<Message>,
}

pub struct Target {
    pub bounds: Rectangle,
}

impl<'a, Message, Theme, Renderer> Simulator<'a, Message, Theme, Renderer>
where
    Theme: Default + theme::Base,
    Renderer: core::Renderer + core::renderer::Headless,
{
    pub fn new(
        element: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self::with_settings(Settings::default(), element)
    }

    pub fn with_settings(
        settings: Settings,
        element: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self::with_settings_and_size(
            settings,
            window::Settings::default().size,
            element,
        )
    }

    pub fn with_settings_and_size(
        settings: Settings,
        window_size: impl Into<Size>,
        element: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        let window_size = window_size.into();

        let default_font = match settings.default_font {
            Font::DEFAULT => Font::with_name("Fira Sans"),
            _ => settings.default_font,
        };

        for font in settings.fonts {
            load_font(font).expect("Font must be valid");
        }

        let mut renderer =
            Renderer::new(default_font, settings.default_text_size);

        let raw = UserInterface::build(
            element,
            window_size,
            user_interface::Cache::default(),
            &mut renderer,
        );

        Simulator {
            raw,
            renderer,
            window_size,
            messages: Vec::new(),
        }
    }

    pub fn find(
        &mut self,
        selector: impl Into<Selector>,
    ) -> Result<Target, Error> {
        let selector = selector.into();

        match &selector {
            Selector::Id(id) => {
                struct FindById<'a> {
                    id: &'a widget::Id,
                    target: Option<Target>,
                }

                impl widget::Operation for FindById<'_> {
                    fn container(
                        &mut self,
                        id: Option<&widget::Id>,
                        bounds: Rectangle,
                        operate_on_children: &mut dyn FnMut(
                            &mut dyn widget::Operation<()>,
                        ),
                    ) {
                        if self.target.is_some() {
                            return;
                        }

                        if Some(self.id) == id {
                            self.target = Some(Target { bounds });
                            return;
                        }

                        operate_on_children(self);
                    }

                    fn scrollable(
                        &mut self,
                        id: Option<&widget::Id>,
                        bounds: Rectangle,
                        _content_bounds: Rectangle,
                        _translation: core::Vector,
                        _state: &mut dyn widget::operation::Scrollable,
                    ) {
                        if self.target.is_some() {
                            return;
                        }

                        if Some(self.id) == id {
                            self.target = Some(Target { bounds });
                        }
                    }

                    fn text_input(
                        &mut self,
                        id: Option<&widget::Id>,
                        bounds: Rectangle,
                        _state: &mut dyn widget::operation::TextInput,
                    ) {
                        if self.target.is_some() {
                            return;
                        }

                        if Some(self.id) == id {
                            self.target = Some(Target { bounds });
                        }
                    }

                    fn text(
                        &mut self,
                        id: Option<&widget::Id>,
                        bounds: Rectangle,
                        _text: &str,
                    ) {
                        if self.target.is_some() {
                            return;
                        }

                        if Some(self.id) == id {
                            self.target = Some(Target { bounds });
                        }
                    }

                    fn custom(
                        &mut self,
                        id: Option<&widget::Id>,
                        bounds: Rectangle,
                        _state: &mut dyn std::any::Any,
                    ) {
                        if self.target.is_some() {
                            return;
                        }

                        if Some(self.id) == id {
                            self.target = Some(Target { bounds });
                        }
                    }
                }

                let mut find = FindById { id, target: None };

                self.raw.operate(&self.renderer, &mut find);

                find.target.ok_or(Error::NotFound(selector))
            }
            Selector::Text(text) => {
                struct FindByText<'a> {
                    text: &'a str,
                    target: Option<Target>,
                }

                impl widget::Operation for FindByText<'_> {
                    fn container(
                        &mut self,
                        _id: Option<&widget::Id>,
                        _bounds: Rectangle,
                        operate_on_children: &mut dyn FnMut(
                            &mut dyn widget::Operation<()>,
                        ),
                    ) {
                        if self.target.is_some() {
                            return;
                        }

                        operate_on_children(self);
                    }

                    fn text(
                        &mut self,
                        _id: Option<&widget::Id>,
                        bounds: Rectangle,
                        text: &str,
                    ) {
                        if self.target.is_some() {
                            return;
                        }

                        if self.text == text {
                            self.target = Some(Target { bounds });
                        }
                    }
                }

                let mut find = FindByText { text, target: None };

                self.raw.operate(&self.renderer, &mut find);

                find.target.ok_or(Error::NotFound(selector))
            }
        }
    }

    pub fn click(
        &mut self,
        selector: impl Into<Selector>,
    ) -> Result<Target, Error> {
        let target = self.find(selector)?;

        let _ = self.raw.update(
            &[
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(target.bounds.center()),
            &mut self.renderer,
            &mut clipboard::Null,
            &mut self.messages,
        );

        Ok(target)
    }

    pub fn typewrite(&mut self, text: impl AsRef<str>) {
        let events: Vec<_> = text
            .as_ref()
            .chars()
            .map(|c| SmolStr::new_inline(&c.to_string()))
            .flat_map(|c| {
                key_press_and_release(
                    keyboard::Key::Character(c.clone()),
                    Some(c),
                )
            })
            .collect();

        let _ = self.raw.update(
            &events,
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut clipboard::Null,
            &mut self.messages,
        );
    }

    pub fn press_key(&mut self, key: impl Into<keyboard::Key>) {
        let _ = self.raw.update(
            &key_press_and_release(key, None),
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut clipboard::Null,
            &mut self.messages,
        );
    }

    pub fn snapshot(&mut self) -> Result<Snapshot, Error> {
        let theme = Theme::default();
        let base = theme.base();

        let _ = self.raw.update(
            &[Event::Window(window::Event::RedrawRequested(
                time::Instant::now(),
            ))],
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut clipboard::Null,
            &mut self.messages,
        );

        let _ = self.raw.draw(
            &mut self.renderer,
            &theme,
            &core::renderer::Style {
                text_color: base.text_color,
            },
            mouse::Cursor::Unavailable,
        );

        let scale_factor = 2.0;

        let physical_size = Size::new(
            (self.window_size.width * scale_factor).round() as u32,
            (self.window_size.height * scale_factor).round() as u32,
        );

        let rgba = self.renderer.screenshot(
            physical_size,
            scale_factor,
            base.background_color,
        );

        Ok(Snapshot {
            screenshot: window::Screenshot::new(
                rgba,
                physical_size,
                f64::from(scale_factor),
            ),
        })
    }

    pub fn into_messages(self) -> impl IntoIterator<Item = Message> {
        self.messages
    }
}

pub struct Snapshot {
    screenshot: window::Screenshot,
}

impl Snapshot {
    pub fn matches_image(&self, path: impl AsRef<Path>) -> Result<bool, Error> {
        let path = path.as_ref().with_extension("png");

        if path.exists() {
            let file = fs::File::open(&path)?;
            let decoder = png::Decoder::new(file);

            let mut reader = decoder.read_info()?;
            let mut bytes = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut bytes)?;

            Ok(self.screenshot.bytes == bytes[..info.buffer_size()])
        } else {
            if let Some(directory) = path.parent() {
                fs::create_dir_all(directory)?;
            }

            let file = fs::File::create(path)?;

            let mut encoder = png::Encoder::new(
                file,
                self.screenshot.size.width,
                self.screenshot.size.height,
            );
            encoder.set_color(png::ColorType::Rgba);

            let mut writer = encoder.write_header()?;
            writer.write_image_data(&self.screenshot.bytes)?;
            writer.finish()?;

            Ok(true)
        }
    }

    pub fn matches_hash(&self, path: impl AsRef<Path>) -> Result<bool, Error> {
        use sha2::{Digest, Sha256};

        let path = path.as_ref().with_extension("sha256");

        let hash = {
            let mut hasher = Sha256::new();
            hasher.update(&self.screenshot.bytes);
            format!("{:x}", hasher.finalize())
        };

        if path.exists() {
            let saved_hash = fs::read_to_string(&path)?;

            Ok(hash == saved_hash)
        } else {
            if let Some(directory) = path.parent() {
                fs::create_dir_all(directory)?;
            }

            fs::write(path, hash)?;
            Ok(true)
        }
    }
}

fn key_press_and_release(
    key: impl Into<keyboard::Key>,
    text: Option<SmolStr>,
) -> [Event; 2] {
    let key = key.into();

    [
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key.clone(),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::default(),
            text,
        }),
        Event::Keyboard(keyboard::Event::KeyReleased {
            key: key.clone(),
            modified_key: key,
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::default(),
        }),
    ]
}

#[derive(Debug, Clone)]
pub enum Error {
    NotFound(Selector),
    IOFailed(Arc<io::Error>),
    PngDecodingFailed(Arc<png::DecodingError>),
    PngEncodingFailed(Arc<png::EncodingError>),
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::IOFailed(Arc::new(error))
    }
}

impl From<png::DecodingError> for Error {
    fn from(error: png::DecodingError) -> Self {
        Self::PngDecodingFailed(Arc::new(error))
    }
}

impl From<png::EncodingError> for Error {
    fn from(error: png::EncodingError) -> Self {
        Self::PngEncodingFailed(Arc::new(error))
    }
}
