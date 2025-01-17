use color_eyre::eyre::Result;
use resvg::render;
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{FitTo, Tree};
use std::ffi::OsString;
use std::fs::create_dir_all;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::{env, fs};
use walkdir::WalkDir;

const ICON_SIZES: &[&str] = &["128x128", "scalable", "256x256", "64x64", "32x32"];

pub fn get_desktop_files() -> Vec<PathBuf> {
	// Get the XDG data directories
	let xdg_data_dirs =
		std::env::var("XDG_DATA_DIRS").unwrap_or("/usr/local/share:/usr/share".to_string());

	// Append the applications directory to each data directory
	let app_dirs = xdg_data_dirs
		.split(":")
		.map(|dir| Path::new(dir).join("applications"));

	// Get the user's local applications directory
	let local_app_dir = dirs::home_dir()
		.unwrap()
		.join(".local")
		.join("share")
		.join("applications");

	let desktop_extension = OsString::from_str("desktop").unwrap();

	// Get the list of directories to search
	app_dirs
		.chain(Some(local_app_dir))
		.filter(|dir| dir.exists() && dir.is_dir())
		.flat_map(|dir| {
			// Follow symlinks and recursively search directories
			WalkDir::new(dir)
				.follow_links(true)
				.into_iter()
				.filter_map(|entry| entry.ok())
				.filter(|entry| entry.file_type().is_file())
				.map(|entry| entry.path().to_path_buf())
		})
		.filter(|path| path.extension() == Some(&desktop_extension))
		.collect::<Vec<PathBuf>>()
}

#[test]
fn test_get_desktop_files() {
	let desktop_files = get_desktop_files();
	dbg!(&desktop_files);
	assert!(desktop_files
		.iter()
		.any(|file| file.ends_with("gimp.desktop")));
}

pub fn parse_desktop_file(path: PathBuf) -> Result<DesktopFile, String> {
	// Open the file in read-only mode
	let file = match fs::File::open(
		env::current_dir()
			.map_err(|e| e.to_string())?
			.join(path.clone()),
	) {
		Ok(file) => file,
		Err(err) => return Err(format!("Failed to open file: {}", err)),
	};

	let reader = BufReader::new(file);

	// Create temporary variables to hold the parsed values
	let mut name = None;
	let mut command = None;
	let mut categories = Vec::new();
	let mut icon = None;

	// Loop through each line of the file
	for line in reader.lines() {
		let line = match line {
			Ok(line) => line,
			Err(err) => return Err(format!("Failed to read line: {}", err)),
		};

		// Skip empty lines and lines that start with "#" (comments)
		if line.is_empty() || line.starts_with('#') {
			continue;
		}

		// Split the line into a key-value pair by looking for the first "=" character
		let parts = line.split_once('=');
		let (key, value) = match parts {
			Some((key, value)) => (key, value),
			None => continue,
		};

		// Parse the key-value pair based on the key
		match key {
			"Name" => name = Some(value.to_string()),
			"Exec" => command = Some(value.to_string()),
			"Categories" => {
				categories = value
					.split(';')
					.map(|s| s.to_string())
					.filter(|s| !s.is_empty())
					.collect()
			}
			"Icon" => icon = Some(value.to_string()),
			_ => (), // Ignore unknown keys
		}
	}

	// Create and return a new DesktopFile instance with the parsed values
	Ok(DesktopFile {
		path,
		name,
		command,
		categories,
		icon,
	})
}

#[test]
fn test_parse_desktop_file() {
	// Create a temporary directory and a test desktop file
	let dir = tempdir::TempDir::new("test").unwrap();
	let file = dir.path().join("test.desktop");
	let data = "[Desktop Entry]\nName=Test\nExec=test\nCategories=A;B;C\nIcon=test.png";
	fs::write(&file, data).unwrap();

	// Parse the test desktop file
	let desktop_file = parse_desktop_file(file).unwrap();

	// Check the parsed values
	assert_eq!(desktop_file.name, Some("Test".to_string()));
	assert_eq!(desktop_file.command, Some("test".to_string()));
	assert_eq!(
		desktop_file.categories,
		vec!["A".to_string(), "B".to_string(), "C".to_string()]
	);
	assert_eq!(desktop_file.icon, Some("test.png".to_string()));
}

#[derive(Debug, Clone)]
pub struct DesktopFile {
	path: PathBuf,
	pub name: Option<String>,
	pub command: Option<String>,
	pub categories: Vec<String>,
	pub icon: Option<String>,
}
impl DesktopFile {
	pub fn get_raw_icons(&self) -> Vec<RawIconType> {
		// Get the name of the icon from the DesktopFile struct
		let Some(icon_name) = self.icon.as_ref() else { return Vec::new(); };
		let test_icon_path = self.path.join(Path::new(icon_name));
		if test_icon_path.exists() {
			return RawIconType::from_path(test_icon_path)
				.map(|i| vec![i])
				.unwrap_or_default();
		}

		let Ok(icon_name) = OsString::from_str(icon_name) else { return Vec::new(); };

		// Get the current icon theme from the XDG_ICON_THEME environment variable, or use "hicolor" as the default theme if the variable is not defined
		let icon_theme = env::var_os("XDG_ICON_THEME").unwrap_or("hicolor".into());

		// Get the XDG_DATA_HOME and XDG_DATA_DIRS environment variables, and split the XDG_DATA_DIRS variable into a list of directories
		let Some(xdg_data_dirs) = env::var_os("XDG_DATA_DIRS") else { return Vec::new(); };
		let Ok(binding) = xdg_data_dirs.into_string() else { return Vec::new(); };
		let xdg_data_dirs = binding.split(":").map(Path::new);

		// Concatenate the XDG_DATA_HOME and XDG_DATA_DIRS directories with the default path for icon themes
		xdg_data_dirs // XDG_DATA_DIRS directories
			.flat_map(|dir| {
				let icons_path = dir.join("icons").join(&icon_theme);
				ICON_SIZES
					.iter()
					.map(|path| icons_path.join(path).join("apps"))
					.collect::<Vec<_>>()
			})
			.filter_map(|dir| {
				let dir = fs::read_dir(dir).ok()?;
				Some(
					dir.filter_map(|e| e.ok())
						.map(|file| file.path())
						.filter(|file| file.file_stem() == Some(&icon_name)),
				)
			})
			.flatten()
			.filter_map(RawIconType::from_path)
			.collect()
	}
}

#[derive(Debug, PartialEq, Eq)]
pub enum RawIconType {
	Png(PathBuf),
	Svg(PathBuf),
	Gltf(PathBuf),
}
impl RawIconType {
	pub fn from_path(path: PathBuf) -> Option<RawIconType> {
		match path.extension().and_then(|ext| ext.to_str()) {
			Some("png") => Some(RawIconType::Png(path)),
			Some("svg") => Some(RawIconType::Svg(path)),
			Some("glb") | Some("gltf") => Some(RawIconType::Gltf(path)),
			_ => None,
		}
	}

	pub fn process(self, size: u32) -> Result<Icon, std::io::Error> {
		match self {
			RawIconType::Png(path) => Ok(Icon::Png(path)),
			RawIconType::Svg(path) => {
				let png_path = path.with_extension("png");
				render_svg_to_png(path, &png_path, size)?;
				Ok(Icon::Png(png_path))
			}
			RawIconType::Gltf(path) => Ok(Icon::Gltf(path)),
		}
	}
}

#[test]
fn test_get_icon_path() {
	// Create an instance of the DesktopFile struct with some dummy data
	let desktop_file = DesktopFile {
		path: PathBuf::new(),
		name: None,
		command: None,
		categories: vec![],
		icon: Some("krita".into()),
	};

	// Call the get_icon_path() function with a size argument and store the result
	let icon_paths = desktop_file.get_raw_icons();
	dbg!(&icon_paths);

	// Assert that the get_icon_path() function returns the expected result
	assert!(icon_paths.contains(&RawIconType::Png(PathBuf::from(
		"/usr/share/icons/hicolor/16x16/apps/krita.png"
	))));
}

#[derive(Debug, PartialEq, Eq)]
pub enum Icon {
	Png(PathBuf),
	Gltf(PathBuf),
}

pub fn render_svg_to_png(
	cache_dir: impl AsRef<Path>,
	svg_path: impl AsRef<Path>,
	size: u32,
) -> Result<PathBuf, std::io::Error> {
	let svg_path = fs::canonicalize(svg_path)?;
	let tree = Tree::from_data(
		fs::read(svg_path.as_path())?.as_slice(),
		&resvg::usvg::Options::default(),
	)
	.map_err(|_| ErrorKind::InvalidData)?;
	create_dir_all(cache_dir.as_ref())?;
	let png_path = cache_dir
		.as_ref()
		.join(svg_path.file_name().unwrap())
		.with_extension("png");
	let mut pixmap = Pixmap::new(size, size).unwrap();
	render(
		&tree,
		FitTo::Width(size),
		Transform::identity(),
		pixmap.as_mut(),
	);
	pixmap
		.save_png(&png_path)
		.map_err(|_| ErrorKind::InvalidData)?;
	Ok(png_path)
}
#[test]
fn test_render_svg_to_png() {
	use image::GenericImageView;
	// Create temporary input and output paths
	let svg_path = env::current_dir().unwrap().join("test_input.svg");

	// Write some test SVG data to the input path
	let test_svg_data = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\">
	<ellipse cx=\"50\" cy=\"80\" rx=\"46\" ry=\"19\" fill=\"#07c\"/>
	<path d=\"M43,0c-6,25,16,22,1,52c11,3,19,0,19-22c38,18,16,63-12,64c-25,2-55-39-8-94\" fill=\"#e34\"/>
	<path d=\"M34,41c-6,39,29,32,33,7c39,42-69,63-33-7\" fill=\"#fc2\"/>
</svg>";
	fs::write(&svg_path, test_svg_data).unwrap();

	// Call the function with the test input and output paths and a size of 200
	let png_path = render_svg_to_png(".", &svg_path, 200).unwrap();
	dbg!(&png_path);

	// Check that the output file exists
	assert!(png_path.exists());

	// Check that the output file is a PNG file
	assert_eq!(png_path.extension().unwrap(), "png");

	// Check that the output file has the expected dimensions
	let output_image = image::open(&png_path).unwrap();
	assert_eq!(output_image.dimensions(), (200, 200));

	// Delete the temporary input and output files
	fs::remove_file(&svg_path).unwrap();
	fs::remove_file(&png_path).unwrap();
}
