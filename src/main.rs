use std::time::Duration;
use tokio::net::{TcpStream, tcp::ReadHalf};
use tokio::io::{self, AsyncReadExt};
use tokio::time::delay_for;
use tokio::sync::mpsc;
use tracing::{Level, debug, error, info, warn};
use tracing_subscriber;
use palette::{Srgb, Yxy};
use palette::named;

mod hue;

// Holds the configuration for the running system. Its global method,
// determine, reads the config file and parses the command line to
// determine the final configuration.

struct Config {
    redis_addr: String,
    redis_port: u16
}

impl Config {
    pub fn determine() -> Option<Config> {
	use clap::{Arg, App};

	// Define the command line arguments.

	let matches = App::new("DrMemory Mini Control System")
            .version("0.1")
            .author("Rich Neswold <rich.neswold@gmail.com>")
            .about("A small, yet capable, control system.")
            .arg(Arg::with_name("config")
		 .short("c")
		 .long("config")
		 .value_name("FILE")
		 .help("Specifies the configuration file")
		 .takes_value(true))
            .arg(Arg::with_name("verbose")
		 .short("v")
		 .long("verbose")
		 .multiple(true)
		 .help("Sets verbosity of log; can be used more than once")
		 .takes_value(false))
            .arg(Arg::with_name("print_cfg")
		 .long("print-config")
		 .help("Displays the configuration and exits")
		 .takes_value(false))
            .get_matches();

	// Return the number of '-v' options to determine the log
	// level.

	let level = match matches.occurrences_of("verbose") {
            0 => Level::WARN,
            1 => Level::INFO,
            2 => Level::DEBUG,
	    _ => Level::TRACE
	};

	// Initialize the log system. The max log level is determined
	// by the user (either through the config file or the command
	// line.)

	let subscriber = tracing_subscriber::fmt()
	    .with_max_level(level.clone())
	    .finish();

	tracing::subscriber::set_global_default(subscriber)
	    .expect("Unable to set global default subscriber");

	info!("logging level set to {}", level);

	// Return the configuration.

	Some(Config { redis_addr: "127.0.0.1".to_string(),
		      redis_port: 6379 })
    }
}

// The sump pump monitor uses a state machine to decide when to
// calculate the duty cycle and in-flow.

#[derive(Debug)]
enum State {
    Unknown,
    Off { off_time: u64 },
    On { off_time: u64, on_time: u64 }
}

// This interface allows a State value to update itself when an event
// occurs.

impl State {

    // This method is called when an off event occurs. The timestamp
    // of the off event needs to be provided. If the state machine has
    // enough information of the previous pump cycle, it will return
    // the duty cycle and in-flow rate. If the state machine is still
    // sync-ing with the state, the state will get updated, but `None`
    // will be returned.

    pub fn to_off(&mut self, stamp: u64) -> Option<(f64, f64)> {
	match *self {
	    State::Unknown => {
		info!("sync-ed with OFF state");
		*self = State::Off { off_time: stamp };
		None
	    },

	    State::Off { off_time: _ } => {
		warn!("ignoring duplicate OFF event");
		None
	    },

	    State::On { off_time, on_time } => {
		let on_time = ((stamp - on_time) as f64) / 1000.0;
		let off_time = ((stamp - off_time) as f64) / 1000.0;
		let duty = (on_time * 100.0 / off_time).round();
		let in_flow = (2680.0 * duty / 60.0).round() / 100.0;

		*self = State::Off { off_time: stamp };
		Some((duty, in_flow))
	    }
	}
    }

    // This method is called when updating the state with an on
    // event. The timestamp of the on event needs to be provided. If
    // the on event actually caused a state change, `true` is
    // returned.

    pub fn to_on(&mut self, stamp: u64) -> bool {
	match *self {
	    State::Unknown => false,

	    State::Off { off_time } => {
		*self = State::On { off_time, on_time: stamp };
		true
	    },

	    State::On { .. } => {
		warn!("ignoring duplicate ON event");
		false
	    }
	}
    }
}

// This function reads the next frame from the sump pump process. It
// either returns `Ok()` with the two fields' values or `Err()` if a
// socket error occurred.

async fn get_reading(rx: &mut ReadHalf<'_>) -> io::Result<(u64, bool)> {
    let stamp = rx.read_u64().await?;
    let value = rx.read_u32().await?;

    return Ok((stamp, value != 0))
}

// Adds a value to "sump:service"'s history.

async fn set_service_state(con: &mut redis::aio::Connection,
			   value: &str) -> redis::RedisResult<()> {
    redis::Cmd::xadd("sump:service.hist", "*", &[("value", value)])
	.query_async(con).await
}

// Returns a connection to the REDIS database. The connection
// infomation is obatined through the current configuration structure.

async fn mk_redis_conn(cfg: &Config)
		       -> redis::RedisResult<redis::aio::Connection> {
    let addr = redis::ConnectionAddr::Tcp(cfg.redis_addr.clone(),
					  cfg.redis_port);
    let info = redis::ConnectionInfo { addr: Box::new(addr),
				       db: 0,
				       username: None,
				       passwd: None };

    debug!("connecting to redis at {}:{}", cfg.redis_addr, cfg.redis_port);
    redis::aio::connect_tokio(&info).await
}

async fn lamp_alert(tx: &mut mpsc::Sender<hue::Program>) -> () {
    let b : Yxy = Srgb::<f32>::from_format(named::BLUE).into_linear().into();
    let r : Yxy = Srgb::<f32>::from_format(named::RED).into_linear().into();
    let prog =
	vec![hue::HueCommands::On { light: 5, bri: 255, color: Some(b) },
	     hue::HueCommands::On { light: 8, bri: 255, color: Some(b) },
	     hue::HueCommands::Pause { len: Duration::from_millis(500) },
	     hue::HueCommands::On { light: 5, bri: 255, color: Some(r) },
	     hue::HueCommands::On { light: 8, bri: 255, color: Some(r) },
	     hue::HueCommands::Pause { len: Duration::from_millis(5_000) },
	     hue::HueCommands::Off { light: 5 },
	     hue::HueCommands::Off { light: 8 }];

    tx.send(prog).await;
}

async fn lamp_off(tx: &mut mpsc::Sender<hue::Program>, duty: f64) -> () {
    let prog = if duty < 10.0 {
	vec![hue::HueCommands::Off { light: 5 },
	     hue::HueCommands::Off { light: 8 }]
    } else {
	let cc = if duty < 30.0 { named::YELLOW } else { named::RED };
	let c : Yxy = Srgb::<f32>::from_format(cc).into_linear().into();

	vec![hue::HueCommands::On { light: 5, bri: 255, color: Some(c) },
	     hue::HueCommands::On { light: 8, bri: 255, color: Some(c) },
	     hue::HueCommands::Pause { len: Duration::from_millis(5_000) },
	     hue::HueCommands::Off { light: 5 },
	     hue::HueCommands::Off { light: 8 }]
    };

    tx.send(prog).await;
}

// Returns an async function which monitors the sump pump, computes
// interesting, related values, and writes these details to associated
// devices' history.

async fn monitor(cfg: &Config,
		 mut tx: mpsc::Sender<hue::Program>) -> redis::RedisResult<()> {
    use std::net::{Ipv4Addr, SocketAddrV4};

    let mut con = mk_redis_conn(cfg).await?;
    let addr = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 101), 10_000);

    let c1 : Yxy = Srgb::<f32>::from_format(named::BLUE)
	.into_linear().into();

    loop {
	let mut state = State::Unknown;

	match TcpStream::connect(addr).await {
	    Ok(mut s) => {
		let (mut rx, _) = s.split();

		set_service_state(&mut con, "up").await?;
		loop {
		    match get_reading(&mut rx).await {
			Ok((stamp, true)) => {
			    if state.to_on(stamp) {
				let sump_on =
				    vec![hue::HueCommands::On { light: 5,
								bri: 255,
								color: Some(c1) },
					 hue::HueCommands::On { light: 8,
								bri: 255,
								color: Some(c1) }];
				tx.send(sump_on).await;
				let _ : () =
				    redis::Cmd::xadd("sump:state.hist",
						     stamp,
						     &[("value", "on")])
				    .query_async(&mut con).await?;
			    }
			},
			Ok((stamp, false)) => {
			    if let Some((duty, in_flow)) = state.to_off(stamp) {
				info!("duty: {}%, in flow: {} gpm", duty, in_flow);

				lamp_off(&mut tx, duty).await;

				let _ : () = redis::pipe()
				    .atomic()
				    .cmd("XADD").arg("sump:state.hist").arg(stamp)
				    .arg("value").arg("off").ignore()
				    .cmd("XADD").arg("sump:duty.hist").arg(stamp)
				    .arg("value").arg(duty).ignore()
				    .cmd("XADD").arg("sump:in-flow.hist").arg(stamp)
				    .arg("value").arg(in_flow)
				    .query_async(&mut con).await?;
			    }
			},
			Err(e) => {
			    error!("couldn't read sump state -- {:?}", e);
			    set_service_state(&mut con, "crash").await?;
			    lamp_alert(&mut tx).await;
			    break;
			}
		    }
		    info!("state: {:?}", state);
		}
	    },
	    Err(e) => {
		set_service_state(&mut con, "down").await?;
		lamp_alert(&mut tx).await;
		error!("couldn't connect to pump process -- {:?}", e)
	    }
	}

	// Delay for 10 seconds before retrying.

	delay_for(Duration::from_millis(10_000)).await;
    }
}

#[tokio::main]
async fn main() -> redis::RedisResult<()> {
    if let Some(cfg) = Config::determine() {
	if let Ok((mut tx, _join)) = hue::manager() {
	    let c1 : Yxy = Srgb::<f32>::from_format(named::RED)
		.into_linear().into();
	    let c2 : Yxy = Srgb::<f32>::from_format(named::WHITE)
		.into_linear().into();
	    let c3 : Yxy = Srgb::<f32>::from_format(named::BLUE)
		.into_linear().into();

	    let prog =
		vec![hue::HueCommands::On{ light: 5, bri: 255, color: Some(c1) },
		     hue::HueCommands::On{ light: 8, bri: 255, color: Some(c1) },
		     hue::HueCommands::Pause { len: Duration::from_millis(1_000) },
		     hue::HueCommands::On{ light: 5, bri: 255, color: Some(c2) },
		     hue::HueCommands::On{ light: 8, bri: 255, color: Some(c2) },
		     hue::HueCommands::Pause { len: Duration::from_millis(1_000) },
		     hue::HueCommands::On{ light: 5, bri: 255, color: Some(c3) },
		     hue::HueCommands::On{ light: 8, bri: 255, color: Some(c3) },
		     hue::HueCommands::Pause { len: Duration::from_millis(1_000) },
		     hue::HueCommands::Off { light: 5 },
		     hue::HueCommands::Off { light: 8 }];

	    tx.send(prog).await;
	    monitor(&cfg, tx).await;
	}
    }
    Ok(())
}
