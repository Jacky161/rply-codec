use std::{path::Path, process::exit};

use retro_rs::Emulator;
use rply_codec::{Counter, Frame, Timer, counts, decode, encode, stats};

fn main() {
    let args: Vec<_> = std::env::args().collect();

    let file =
        std::fs::File::open(args.get(1).unwrap_or(&"examples/bobl_new.replay".to_string())).unwrap();
    let outfile = std::fs::File::create(
        args.get(2)
            .unwrap_or(&"examples/bobl_new_trim.replay".to_string()),
    )
    .unwrap();
    let start_frame: Option<u64> = args.get(3).and_then(|arg| arg.parse::<u64>().ok());
    let end_frame: Option<u64> = args.get(4).and_then(|arg| arg.parse::<u64>().ok());

    // Initialize emulator
    let romfile = args.get(5).unwrap_or(&"roms/bobl.nes".to_string()).clone();
    let corefile = args
        .get(6)
        .unwrap_or(&"cores/fceumm_libretro.x86_64.so".to_string())
        .clone();
    let mut emu = Emulator::create(Path::new(&corefile), Path::new(&romfile));

    let file = std::io::BufReader::new(file);
    let mut outfile = std::io::BufWriter::new(outfile);
    let mut rply = decode(file).unwrap();
    let header = &rply.header;
    println!("{header:?}");
    if header.version() == 0 {
        println!("Can't trim v0 replays, upgrade to v1 first using upgrade0");
        std::process::exit(-1);
    }

    let mut header_out = header.clone();
    header_out.upgrade();
    header_out.set_block_size(128);
    header_out.set_superblock_size(128);

    let mut frame = Frame::default();
    let mut initial_state: Vec<u8> = rply.initial_state.clone();

    // run emu a tick to make sure we have right frame sizes, etc
    emu.run([retro_rs::Buttons::default(); 2]);
    assert!(emu.load(&initial_state));

    // Seek to start frame if one was provided
    if let Some(start_frame) = start_frame
        && start_frame > 0
    {
        println!("Seeking to start frame {}", start_frame);
        for _ in 0..start_frame {
            let Ok(()) = rply
                .read_frame(&mut frame)
                .inspect_err(|e| println!("Err: {e}"))
            else {
                // If start frame is too big
                exit(1);
            };

            println!(
                " {}{:08}",
                if frame.checkpoint_bytes.is_empty() {
                    " "
                } else {
                    "*"
                },
                rply.frame_number,
            );

            // Run emulator
            let buttons = frame_to_buttons(&frame);
            emu.run(buttons);

            // Load checkpoint if present
            if !frame.checkpoint_bytes.is_empty() {
                assert!(emu.load(&frame.checkpoint_bytes));
            }
        }

        if !frame.checkpoint_bytes.is_empty() {
            initial_state = frame.checkpoint_bytes.clone();
        } else {
            initial_state = vec![0; emu.save_size()];
            assert!(emu.save(&mut initial_state));
        }
    }

    // Copy rest of the frames into the output replay
    let mut out = encode(header_out, &initial_state, &mut outfile).unwrap();
    while let Ok(()) = rply
        .read_frame(&mut frame)
        .inspect_err(|e| println!("Err: {e}"))
    {
        println!(
            " {}{:08} {}",
            if frame.checkpoint_bytes.is_empty() {
                " "
            } else {
                "*"
            },
            rply.frame_number,
            frame.inputs(),
        );

        // TODO run libretro core here, maybe serialize into frame.checkpoint_bytes
        // TODO maybe get screenshot or add to video

        out.write_frame(&frame).unwrap();
        if Some(rply.frame_number) == end_frame
            || Some(rply.frame_number) == rply.header.frame_count()
        {
            break;
        }
    }

    out.finish().unwrap();
    for timer in [
        Timer::DecodeFrame,
        Timer::DecodeCheckpoint,
        Timer::DecodeStatestream,
        Timer::EncodeFrame,
        Timer::EncodeCheckpoint,
        Timer::EncodeStatestream,
    ] {
        let times = stats(timer);
        #[allow(clippy::cast_precision_loss)]
        let avg_time = (times.micros as f64 / times.count as f64) / 1000.0;
        println!("{timer:?}: {} ({avg_time:.8}ms avg)", times.count);
    }
    for counter in [
        Counter::DecSkippedSuperblocks,
        Counter::DecSkippedBlocks,
        Counter::EncReusedBlocks,
        Counter::EncReusedSuperblocks,
        Counter::EncSkippedBlocks,
        Counter::EncMemCmps,
        Counter::EncHashes,
        Counter::EncTotalBlocks,
        Counter::EncTotalSuperblocks,
        Counter::EncTotalKBsIn,
        Counter::EncTotalKBsOut,
    ] {
        println!("{counter:?}: {}", counts(counter));
    }
}

fn frame_to_buttons(frame: &Frame) -> [retro_rs::Buttons; 2] {
    use retro_rs::Buttons;
    let mut buttons = [0_i16; 2];
    for inp in &frame.input_events {
        let port = usize::from(inp.port);
        if port < buttons.len() && inp.device == 1 {
            buttons[port] |= inp.val;
        }
    }
    [Buttons::from(buttons[0]), Buttons::from(buttons[1])]
}
