mod comic;
mod config;
mod convert;
mod db;
mod error;
mod kf8_extract;
mod kindle;
mod library;
mod metadata;
mod openlibrary;
mod reader;
mod state;
mod ui;

pub fn run() {
    ui::launch();
}
