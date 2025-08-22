
use rayon::prelude::*;
use rusqlite::{params, Connection, Result};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;
use image::GenericImageView;

#[derive(Debug)]
struct ImageData {
    name: String,
    path: String,
    http: String,
    idx: usize,
    orientation: String,
    width: u32,
    height: u32,
}

fn img_orient(img_path: &str) -> Result<(u32, u32, String), String> {
    match image::open(img_path) {
        Ok(img) => {
            let (width, height) = img.dimensions();
            let orientation = if width > height {
                "landscape"
            } else if width < height {
                "portrait"
            } else {
                "square"
            };
            Ok((width, height, orientation.to_string()))
        }
        Err(e) => Err(format!("Error processing image {}: {}", img_path, e)),
    }
}

fn create_img_db_table(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS images (
            Name TEXT,
            Path TEXT,
            Http TEXT,
            Idx INTEGER,
            Orientation TEXT,
            Width INTEGER,
            Height INTEGER
        );",
        [],
    )?;
    Ok(())
}

fn create_http_path(fpath: &str) -> String {
    fpath.replace("/home/pi/Pictures/test2/", "/static/")
}

fn walk_img_dir(conn: &mut Connection, directory: &str) {
    let mut failed_images = Vec::new();

    // Collect all jpg files first for parallel processing
    let files: Vec<_> = WalkDir::new(directory)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().map(|ext| ext.eq_ignore_ascii_case("jpg")).unwrap_or(false)
        })
        .collect();

    let results: Vec<_> = files
        .par_iter()
        .enumerate()
        .map(|(i, entry)| {
            let file_path = entry.path().to_string_lossy().to_string();
            let file_name = entry.file_name().to_string_lossy().to_string();
            match img_orient(&file_path) {
                Ok((width, height, orientation)) => {
                    let image_data = ImageData {
                        name: file_name,
                        path: file_path.clone(),
                        http: create_http_path(&file_path),
                        idx: i + 1,
                        orientation,
                        width,
                        height,
                    };
                    Some(Ok(image_data))
                }
                Err(e) => Some(Err((file_path, e))),
            }
        })
        .filter_map(|x| x)
        .collect();

    let tx = conn.transaction().unwrap();
    for res in results {
        match res {
            Ok(image_data) => {
                println!("{:?}", image_data);
                let _ = tx.execute(
                    "INSERT INTO images (Name, Path, Http, Idx, Orientation, Width, Height) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        image_data.name,
                        image_data.path,
                        image_data.http,
                        image_data.idx as i64,
                        image_data.orientation,
                        image_data.width as i64,
                        image_data.height as i64
                    ],
                );
            }
            Err((file_path, err_msg)) => {
                println!("Skipping image {}: {}", file_path, err_msg);
                failed_images.push(file_path);
            }
        }
    }
    tx.commit().unwrap();

    // Print summary
    println!("\n--- Summary ---");
    if !failed_images.is_empty() {
        println!("Failed to process {} image(s):", failed_images.len());
        for img in failed_images {
            println!("  - {}", img);
        }
    } else {
        println!("All images processed successfully!");
    }
}

fn main() {
    let db_path = "/home/pi/go/slideshowgodocker/DB/imagesDB";
    let image_dir = "/home/pi/Pictures/test2/";

    // Ensure DB directory exists
    if let Some(parent) = Path::new(db_path).parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).expect("Failed to create DB directory");
        }
    }

    let mut conn = Connection::open(db_path).expect("Failed to open DB");
    create_img_db_table(&conn).expect("Failed to create table");
    walk_img_dir(&mut conn, image_dir);
}
