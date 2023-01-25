use color_eyre::eyre::Result;
use manifest_dir_macros::directory_relative_path;
use mint::Vector3;
use protostar::{
	protostar::ProtoStar,
	xdg::{get_desktop_files, parse_desktop_file, DesktopFile},
};
use stardust_xr_molecules::fusion::{
	client::{Client, LifeCycleHandler, LogicStepInfo},
	spatial::Spatial,
};

const APP_LIMIT: usize = 50;
const APP_SIZE: f32 = 0.05;
const GRID_PADDING: f32 = 0.01;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
	color_eyre::install().unwrap();
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		.pretty()
		.init();
	let (client, event_loop) = Client::connect_with_async_loop().await?;
	client.set_base_prefixes(&[directory_relative_path!("res")]);

	let _root = client.wrap_root(AppGrid::new(&client));

	tokio::select! {
		_ = tokio::signal::ctrl_c() => (),
		e = event_loop => e??,
	};
	Ok(())
}

struct AppGrid {
	apps: Vec<App>,
}
impl AppGrid {
	fn new(client: &Client) -> Self {
		let apps = get_desktop_files()
			.into_iter()
			.filter_map(|d| parse_desktop_file(d).ok())
			.enumerate()
			.filter(|(i, _)| *i <= APP_LIMIT)
			.filter_map(|(i, a)| {
				App::new(
					client.get_root(),
					[
						(i % 10) as f32 * (APP_SIZE + GRID_PADDING),
						(i / 10) as f32 * (APP_SIZE + GRID_PADDING),
						0.0,
					],
					a,
				)
			})
			.collect::<Vec<_>>();
		AppGrid { apps }
	}
}
impl LifeCycleHandler for AppGrid {
	fn logic_step(&mut self, info: LogicStepInfo) {
		for app in &mut self.apps {
			app.logic_step(info);
		}
	}
}
struct App {
	// _text: Text,
	_desktop_file: DesktopFile,
	protostar: ProtoStar,
}
impl App {
	fn new(
		parent: &Spatial,
		position: impl Into<Vector3<f32>>,
		desktop_file: DesktopFile,
	) -> Option<Self> {
		let position = position.into();

		let protostar = ProtoStar::create_from_desktop_file(parent, desktop_file.clone()).ok()?;
		// let text = Text::create(
		// 	protostar.content_parent(),
		// 	Transform::from_position_rotation(
		// 		[0.0, 0.0, APP_SIZE / 2.0],
		// 		Quat::from_rotation_y(PI),
		// 	),
		// 	desktop_file.name.as_deref().unwrap_or("Unknown"),
		// 	TextStyle {
		// 		character_height: APP_SIZE * 0.1,
		// 		bounds: Some(Bounds {
		// 			bounds: [APP_SIZE; 2].into(),
		// 			fit: TextFit::Wrap,
		// 			bounds_align: Alignment::XCenter | Alignment::YCenter,
		// 		}),
		// 		text_align: Alignment::XCenter | Alignment::YCenter,
		// 		..Default::default()
		// 	},
		// )
		// .unwrap();
		protostar
			.content_parent()
			.set_position(None, position)
			.unwrap();
		Some(App {
			// _text: text,
			_desktop_file: desktop_file,
			protostar,
		})
	}
}
impl LifeCycleHandler for App {
	fn logic_step(&mut self, info: LogicStepInfo) {
		self.protostar.logic_step(info);
	}
}