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

use aimdb_core::AimDbBuilder;
use aimdb_embassy_adapter::{
    EmbassyAdapter, EmbassyBufferType, EmbassyRecordRegistrarExt, EmbassyRecordRegistrarExtCustom,
};
use aimdb_knx_connector::embassy_client::KnxConnectorBuilder;
use aimdb_mqtt_connector::embassy_client::MqttConnectorBuilder;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_stm32::eth::{Ethernet, GenericPhy, PacketQueue};
use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::peripherals::ETH;
use embassy_stm32::rng::Rng;
use embassy_stm32::{Config, bind_interrupts, eth, peripherals, rng};
use embassy_time::{Duration, Timer};
use records::{SwitchControl, SwitchState, Temperature};
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

    // Configure SwitchState record (inbound: KNX → AimDB, outbound: AimDB → MQTT)
    builder.configure::<SwitchState>(|reg| {
        reg.buffer_sized::<8, 2>(EmbassyBufferType::SingleLatest)
            .tap(records::switch::monitors::state_monitor)
            // Subscribe from KNX group address 1/0/7 (switch monitoring)
            .link_from("knx://1/0/7")
            .with_deserializer(|data: &[u8]| {
                records::switch::knx::deserialize_switch_state_from_knx(data, "1/0/7")
            })
            .finish()
            // Publish to MQTT as JSON
            .link_to(SwitchState::MQTT_TOPIC)
            .with_serializer(|state: &SwitchState| {
                records::switch::serde::serialize_state(state)
                    .map_err(|_| aimdb_core::connector::SerializeError::InvalidData)
            })
            .finish();
    });

    // Configure Temperature record (inbound: KNX → AimDB, outbound: AimDB → MQTT)
    builder.configure::<Temperature>(|reg| {
        reg.buffer_sized::<8, 2>(EmbassyBufferType::SingleLatest)
            .tap(records::temperature::monitors::monitor)
            // Subscribe from KNX temperature sensor (group address 9/1/0)
            .link_from("knx://9/1/0")
            .with_deserializer(|data: &[u8]| records::temperature::knx::from_knx(data, "9/1/0"))
            .finish()
            // Publish to MQTT as JSON
            .link_to(Temperature::MQTT_TOPIC)
            .with_serializer(|temp: &Temperature| {
                records::temperature::serde::serialize(temp)
                    .map_err(|_| aimdb_core::connector::SerializeError::InvalidData)
            })
            .finish();
    });

    // Configure SwitchControl record (inbound: MQTT → AimDB, outbound: AimDB → KNX)
    builder.configure::<SwitchControl>(|reg| {
        reg.buffer_sized::<8, 2>(EmbassyBufferType::SingleLatest)
            .tap(records::switch::monitors::control_monitor)
            // Subscribe from MQTT commands
            .link_from(SwitchControl::MQTT_TOPIC)
            .with_deserializer(|data: &[u8]| records::switch::serde::deserialize_control(data))
            .finish()
            // Publish to KNX group address 1/0/6 (switch control)
            .link_to("knx://1/0/6")
            .with_serializer(|control: &SwitchControl| {
                records::switch::knx::serialize_switch_control_to_knx(control)
                    .map_err(|_| aimdb_core::connector::SerializeError::InvalidData)
            })
            .finish();
    });

    info!("✅ Database configured with KNX and MQTT bridge:");
    info!("   KNX INBOUND (KNX → AimDB → MQTT):");
    info!(
        "     - knx://1/0/7 → {} (DPT 1.001)",
        SwitchState::MQTT_TOPIC
    );
    info!(
        "     - knx://9/1/0 → {} (DPT 9.001)",
        Temperature::MQTT_TOPIC
    );
    info!("   MQTT INBOUND (MQTT → AimDB → KNX):");
    info!(
        "     - {} → knx://1/0/6 (JSON → DPT 1.001)",
        SwitchControl::MQTT_TOPIC
    );
    info!("   KNX Gateway: {}:{}", KNX_GATEWAY_IP, KNX_GATEWAY_PORT);
    info!("   MQTT Broker: {}:{}", MQTT_BROKER_IP, MQTT_BROKER_PORT);
    info!("");
    info!("💡 MQTT commands:");
    info!(
        "   Subscribe: mosquitto_sub -h {} -t 'knx/#' -v",
        MQTT_BROKER_IP
    );
    info!(
        "   Control: mosquitto_pub -h {} -t 'knx/lights/control' \\",
        MQTT_BROKER_IP
    );
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
