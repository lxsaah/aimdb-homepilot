#![no_std]
#![no_main]

//! KNX Gateway for STM32
//!
//! An aimdb-based KNX/IP gateway that bridges KNX devices to MQTT.
//! This gateway:
//! - Connects to KNX bus via KNX/IP protocol
//! - Publishes device states to MQTT broker
//! - Receives commands from MQTT and forwards to KNX bus
//! - Runs on STM32H563ZI microcontroller with Embassy async runtime

extern crate alloc;

use aimdb_core::{AimDbBuilder, Consumer, RuntimeContext};
use aimdb_embassy_adapter::{
    EmbassyAdapter, EmbassyBufferType, EmbassyRecordRegistrarExt, EmbassyRecordRegistrarExtCustom,
};
use aimdb_knx_connector::dpt::{Dpt1, Dpt9, DptDecode, DptEncode};
use aimdb_knx_connector::embassy_client::KnxConnectorBuilder;
use aimdb_mqtt_connector::embassy_client::MqttConnectorBuilder;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_stm32::eth::{Ethernet, GenericPhy, PacketQueue};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::peripherals::ETH;
use embassy_stm32::rng::Rng;
use embassy_stm32::{bind_interrupts, eth, peripherals, rng, Config};
use embassy_time::{Duration, Timer};
use heapless::String as HeaplessString;
use static_cell::StaticCell;
use {defmt_rtt as _, panic_probe as _};

// Simple embedded allocator (required by some dependencies)
#[global_allocator]
static ALLOCATOR: embedded_alloc::Heap = embedded_alloc::Heap::empty();

// Interrupt bindings for Ethernet and RNG
bind_interrupts!(struct Irqs {
    ETH => eth::InterruptHandler;
    RNG => rng::InterruptHandler<peripherals::RNG>;
});

type Device =
    Ethernet<'static, ETH, GenericPhy<embassy_stm32::eth::Sma<'static, peripherals::ETH_SMA>>>;

/// Network task that runs the embassy-net stack
#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, Device>) -> ! {
    runner.run().await
}

// ============================================================================
// KNX DATA TYPES
// ============================================================================

/// Light state from KNX bus (DPT 1.001)
#[derive(Clone, Debug)]
struct LightState {
    group_address: HeaplessString<16>, // "1/0/7"
    is_on: bool,
    #[allow(dead_code)]
    timestamp: u32,
}

/// Temperature from KNX bus (DPT 9.001)
#[derive(Clone, Debug)]
struct Temperature {
    group_address: HeaplessString<16>, // "9/1/0"
    celsius: f32,
    #[allow(dead_code)]
    timestamp: u32,
}

/// Light control command to send to KNX bus (DPT 1.001)
#[derive(Clone, Debug)]
struct LightControl {
    #[allow(dead_code)]
    group_address: HeaplessString<16>, // "1/0/6"
    is_on: bool,
    #[allow(dead_code)]
    timestamp: u32,
}

/// Consumer that logs incoming KNX light telegrams
async fn light_monitor(
    ctx: RuntimeContext<EmbassyAdapter>,
    consumer: Consumer<LightState, EmbassyAdapter>,
) {
    let log = ctx.log();

    log.info("💡 Light monitor started - watching KNX bus...\n");

    let Ok(mut reader) = consumer.subscribe() else {
        log.error("Failed to subscribe to light state buffer");
        return;
    };

    while let Ok(state) = reader.recv().await {
        log.info(&alloc::format!(
            "💡 KNX light: {} = {}",
            state.group_address.as_str(),
            if state.is_on { "ON ✨" } else { "OFF" }
        ));
    }
}

/// Consumer that logs incoming KNX temperature telegrams
async fn temperature_monitor(
    ctx: RuntimeContext<EmbassyAdapter>,
    consumer: Consumer<Temperature, EmbassyAdapter>,
) {
    let log = ctx.log();

    log.info("🌡️  Temperature monitor started - watching KNX bus...\n");

    let Ok(mut reader) = consumer.subscribe() else {
        log.error("Failed to subscribe to temperature buffer");
        return;
    };

    while let Ok(temp) = reader.recv().await {
        log.info(&alloc::format!(
            "🌡️  KNX temperature: {} = {:.1}°C",
            temp.group_address.as_str(),
            temp.celsius
        ));
    }
}

/// KNX/IP gateway IP address
const KNX_GATEWAY_IP: &str = "192.168.1.19";
/// KNX/IP gateway port
const KNX_GATEWAY_PORT: u16 = 3671;

/// MQTT broker IP address
const MQTT_BROKER_IP: &str = "192.168.1.7";
/// MQTT broker port
const MQTT_BROKER_PORT: u16 = 1883;

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize heap for the allocator
    {
        use core::mem::MaybeUninit;
        const HEAP_SIZE: usize = 32768; // 32KB heap
        static mut HEAP: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];
        unsafe {
            let heap_ptr = core::ptr::addr_of_mut!(HEAP);
            ALLOCATOR.init((*heap_ptr).as_ptr() as usize, HEAP_SIZE)
        }
    }

    info!("🚀 Starting KNX Gateway");

    // Configure MCU clocks for STM32H563ZI (from official embassy example)
    let mut config = Config::default();
    {
        use embassy_stm32::rcc::*;
        use embassy_stm32::time::Hertz;

        config.rcc.hsi = None;
        config.rcc.hsi48 = Some(Default::default()); // needed for RNG
        config.rcc.hse = Some(Hse {
            freq: Hertz(8_000_000),
            mode: HseMode::BypassDigital,
        });
        config.rcc.pll1 = Some(Pll {
            source: PllSource::HSE,
            prediv: PllPreDiv::DIV2,
            mul: PllMul::MUL125,
            divp: Some(PllDiv::DIV2),
            divq: Some(PllDiv::DIV2),
            divr: None,
        });
        config.rcc.ahb_pre = AHBPrescaler::DIV1;
        config.rcc.apb1_pre = APBPrescaler::DIV2;
        config.rcc.apb2_pre = APBPrescaler::DIV2;
        config.rcc.apb3_pre = APBPrescaler::DIV2;
        config.rcc.sys = Sysclk::PLL1_P;
        config.rcc.voltage_scale = VoltageScale::Scale0;
    }

    let p = embassy_stm32::init(config);

    info!("✅ MCU initialized");

    // Setup LED for visual feedback (green LED on Nucleo)
    let mut led = Output::new(p.PB0, Level::Low, Speed::Low);

    // Generate random seed for network stack
    let mut rng = Rng::new(p.RNG, Irqs);
    let mut seed = [0; 8];
    rng.fill_bytes(&mut seed);
    let seed = u64::from_le_bytes(seed);

    info!("🔧 Initializing Ethernet...");

    // MAC address for this device
    let mac_addr = [0x00, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];

    // Create Ethernet device
    static PACKETS: StaticCell<PacketQueue<4, 4>> = StaticCell::new();

    let device = Ethernet::new(
        PACKETS.init(PacketQueue::<4, 4>::new()),
        p.ETH,
        Irqs,
        p.PA1,  // ETH_REF_CLK
        p.PA7,  // ETH_CRS_DV
        p.PC4,  // ETH_RXD0
        p.PC5,  // ETH_RXD1
        p.PG13, // ETH_TXD0
        p.PB15, // ETH_TXD1
        p.PG11, // ETH_TX_EN
        mac_addr,
        p.ETH_SMA, // SMA peripheral (replaces old SMA pin)
        p.PA2,     // ETH_MDIO
        p.PC1,     // ETH_MDC
    );

    // Network configuration (using DHCP)
    let config = embassy_net::Config::dhcpv4(Default::default());

    // Initialize network stack
    static RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();
    static STACK_CELL: StaticCell<embassy_net::Stack<'static>> = StaticCell::new();

    let (stack_obj, runner) =
        embassy_net::new(device, config, RESOURCES.init(StackResources::new()), seed);

    let stack: &'static _ = STACK_CELL.init(stack_obj);

    // Spawn network task
    let token = net_task(runner).unwrap();
    spawner.spawn(token);

    info!("⏳ Waiting for network configuration (DHCP)...");

    // Wait for DHCP to complete and network to be ready
    stack.wait_config_up().await;

    info!("✅ Network ready!");
    if let Some(config) = stack.config_v4() {
        info!("   IP address: {}", config.address);
    }


    // Create AimDB database with Embassy adapter
    let runtime = alloc::sync::Arc::new(EmbassyAdapter::new_with_network(spawner, stack));

    info!("🔧 Creating database with KNX bus monitor and MQTT bridge...");

    // Build KNX gateway URL and MQTT broker URL
    use alloc::format;
    let gateway_url = format!("knx://{}:{}", KNX_GATEWAY_IP, KNX_GATEWAY_PORT);
    let broker_url = format!("mqtt://{}:{}", MQTT_BROKER_IP, MQTT_BROKER_PORT);

    info!("📋 Configuring connectors...");
    info!("   KNX Gateway: {}", gateway_url.as_str());
    info!("   MQTT Broker: {}", broker_url.as_str());

    let mut builder = AimDbBuilder::new()
        .runtime(runtime.clone())
        .with_connector(KnxConnectorBuilder::new(&gateway_url))
        .with_connector(MqttConnectorBuilder::new(&broker_url).with_client_id("knx-gateway-001"));

    // Configure LightState record (inbound: KNX → AimDB, outbound: AimDB → MQTT)
    builder.configure::<LightState>(|reg| {
        reg.buffer_sized::<8, 2>(EmbassyBufferType::SingleLatest)
            .tap(light_monitor)
            // Subscribe from KNX group address 1/0/7 (light switch monitoring)
            .link_from("knx://1/0/7")
            .with_deserializer(|data: &[u8]| {
                // Use DPT 1.001 (Switch) to decode boolean value
                let is_on = Dpt1::Switch.decode(data).unwrap_or(false);
                let mut group_address = HeaplessString::<16>::new();
                let _ = group_address.push_str("1/0/7");

                Ok(LightState {
                    group_address,
                    is_on,
                    timestamp: 0,
                })
            })
            .finish()
            // Publish to MQTT as JSON
            .link_to("mqtt://knx/lights/state")
            .with_serializer(|state: &LightState| {
                use alloc::format;
                let json = format!(
                    r#"{{"group_address":"{}","is_on":{},"timestamp":{}}}"#,
                    state.group_address.as_str(),
                    state.is_on,
                    state.timestamp
                );
                Ok(json.into_bytes())
            })
            .finish();
    });

    // Configure Temperature record (inbound: KNX → AimDB, outbound: AimDB → MQTT)
    builder.configure::<Temperature>(|reg| {
        reg.buffer_sized::<8, 2>(EmbassyBufferType::SingleLatest)
            .tap(temperature_monitor)
            // Subscribe from KNX temperature sensor (group address 9/1/0)
            .link_from("knx://9/1/0")
            .with_deserializer(|data: &[u8]| {
                // Use DPT 9.001 (Temperature) to decode 2-byte float temperature
                let celsius = Dpt9::Temperature.decode(data).unwrap_or(0.0);
                let mut group_address = HeaplessString::<16>::new();
                let _ = group_address.push_str("9/1/0");

                Ok(Temperature {
                    group_address,
                    celsius,
                    timestamp: 0,
                })
            })
            .finish()
            // Publish to MQTT as JSON
            .link_to("mqtt://knx/temperature/state")
            .with_serializer(|temp: &Temperature| {
                use alloc::format;
                let json = format!(
                    r#"{{"group_address":"{}","celsius":{:.2},"timestamp":{}}}"#,
                    temp.group_address.as_str(),
                    temp.celsius,
                    temp.timestamp
                );
                Ok(json.into_bytes())
            })
            .finish();
    });

    // Configure LightControl record (inbound: MQTT → AimDB, outbound: AimDB → KNX)
    builder.configure::<LightControl>(|reg| {
        reg.buffer_sized::<8, 2>(EmbassyBufferType::SingleLatest)
            .tap(|ctx: RuntimeContext<EmbassyAdapter>, consumer: Consumer<LightControl, EmbassyAdapter>| async move {
                let log = ctx.log();
                log.info("📥 MQTT→KNX command monitor started...");
                
                let Ok(mut reader) = consumer.subscribe() else {
                    log.error("Failed to subscribe to LightControl buffer");
                    return;
                };
                
                while let Ok(cmd) = reader.recv().await {
                    log.info(&alloc::format!(
                        "📥 MQTT command → KNX: {} = {}",
                        cmd.group_address.as_str(),
                        if cmd.is_on { "ON" } else { "OFF" }
                    ));
                }
            })
            // Subscribe from MQTT commands
            .link_from("mqtt://knx/lights/control")
            .with_deserializer(|data: &[u8]| {
                // Parse JSON command: {"group_address":"1/0/6","is_on":true}
                let text = core::str::from_utf8(data)
                    .map_err(|_| alloc::string::String::from("Invalid UTF-8"))?;
                
                let mut group_address = HeaplessString::<16>::new();
                let mut is_on = false;
                
                // Simple JSON parsing for {"group_address":"xxx","is_on":yyy}
                for pair in text.trim_matches(|c| c == '{' || c == '}').split(',') {
                    let parts: alloc::vec::Vec<&str> = pair.split(':').collect();
                    if parts.len() != 2 {
                        continue;
                    }
                    let key = parts[0].trim().trim_matches('"');
                    let value = parts[1].trim();
                    
                    match key {
                        "group_address" => {
                            let addr = value.trim_matches('"');
                            let _ = group_address.push_str(addr);
                        }
                        "is_on" => {
                            is_on = value == "true";
                        }
                        _ => {}
                    }
                }
                
                Ok(LightControl {
                    group_address,
                    is_on,
                    timestamp: 0,
                })
            })
            .finish()
            // Publish to KNX group address 1/0/6 (light control)
            .link_to("knx://1/0/6")
            .with_serializer(|state: &LightControl| {
                // Use DPT 1.001 (Switch) to encode boolean value
                let mut buf = [0u8; 1];
                let len = Dpt1::Switch.encode(state.is_on, &mut buf).unwrap_or(0);
                Ok(buf[..len].to_vec())
            })
            .finish();
    });

    info!("✅ Database configured with KNX and MQTT bridge:");
    info!("   KNX INBOUND (KNX → AimDB → MQTT):");
    info!("     - knx://1/0/7 → mqtt://knx/lights/state (DPT 1.001)");
    info!("     - knx://9/1/0 → mqtt://knx/temperature/state (DPT 9.001)");
    info!("   MQTT INBOUND (MQTT → AimDB → KNX):");
    info!("     - mqtt://knx/lights/control → knx://1/0/6 (JSON → DPT 1.001)");
    info!("   KNX Gateway: {}:{}", KNX_GATEWAY_IP, KNX_GATEWAY_PORT);
    info!("   MQTT Broker: {}:{}", MQTT_BROKER_IP, MQTT_BROKER_PORT);
    info!("");
    info!("💡 MQTT commands:");
    info!("   Subscribe: mosquitto_sub -h {} -t 'knx/#' -v", MQTT_BROKER_IP);
    info!("   Control: mosquitto_pub -h {} -t 'knx/lights/control' \\", MQTT_BROKER_IP);
    info!("            -m '{{\"group_address\":\"1/0/6\",\"is_on\":true}}'");
    info!("");

    info!("🔨 Building database...");
    static DB_CELL: StaticCell<aimdb_core::AimDb<EmbassyAdapter>> = StaticCell::new();
    let _db = DB_CELL.init(builder.build().await.expect("Failed to build database"));

    info!("✅ Database running with KNX and MQTT connectors");
    info!("🎯 Gateway ready!");
    info!("📡 Bridging KNX ↔ MQTT via Ethernet");
    info!("");

    // Main loop - blink LED to show system is alive
    loop {
        led.set_high();
        Timer::after(Duration::from_millis(100)).await;
        led.set_low();
        Timer::after(Duration::from_millis(900)).await;
    }
}
