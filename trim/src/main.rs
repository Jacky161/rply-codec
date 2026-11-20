use std::process::exit;

use rply_codec::{Counter, Frame, Timer, counts, decode, encode, stats};

fn main() {
    let args: Vec<_> = std::env::args().collect();

    let file = std::fs::File::open(
        args.get(1)
            .unwrap_or(&"examples/bobl.replay".to_string()),
    )
    .unwrap();
    let outfile = std::fs::File::create(
        args.get(2)
            .unwrap_or(&"examples/bobl_trim.replay".to_string()),
    )
    .unwrap();
    let start_frame: Option<u64> = args.get(3).and_then(|arg| arg.parse::<u64>().ok());
    let end_frame: Option<u64> = args.get(4).and_then(|arg| arg.parse::<u64>().ok());

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
    let mut initial_state: &Vec<u8> = &rply.initial_state;

    // Seek to start frame if one was provided
    if let Some(start_frame) = start_frame && start_frame > 0 {
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
        }

        initial_state = &frame.checkpoint_bytes;
        if initial_state.is_empty() {
            println!("Err: no checkpoint exists for start frame {}", start_frame);
            exit(1);
        }
    }

    // Copy rest of the frames into the output replay
    let mut out = encode(header_out, initial_state, &mut outfile).unwrap();
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

        frame.drop_checkpoint();
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
