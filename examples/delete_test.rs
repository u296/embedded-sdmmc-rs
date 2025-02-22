//! # Tests the Embedded SDMMC Library
//! ```bash
//! $ cargo run --example delete_test -- /dev/mmcblk0
//! $ cargo run --example delete_test -- /dev/sda
//! ```
//!
//! If you pass a block device it should be unmounted. No testing has been
//! performed with Windows raw block devices - please report back if you try
//! this! There is a gzipped example disk image which you can gunzip and test
//! with if you don't have a suitable block device.
//!
//! ```bash
//! zcat ./disk.img.gz > ./disk.img
//! $ cargo run --example delete_test -- ./disk.img
//! ```

extern crate embedded_sdmmc;

const FILE_TO_DELETE: &'static str = "DELETE.TXT";

use embedded_sdmmc::{
    Block, BlockCount, BlockDevice, BlockIdx, Controller, Error, Mode, TimeSource, Timestamp,
    VolumeIdx,
};
use std::cell::RefCell;
use std::fs::{File, OpenOptions};
use std::io::prelude::*;
use std::io::SeekFrom;
use std::path::Path;

#[derive(Debug)]
struct LinuxBlockDevice {
    file: RefCell<File>,
    print_blocks: bool,
}

impl LinuxBlockDevice {
    fn new<P>(device_name: P, print_blocks: bool) -> Result<LinuxBlockDevice, std::io::Error>
    where
        P: AsRef<Path>,
    {
        Ok(LinuxBlockDevice {
            file: RefCell::new(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(device_name)?,
            ),
            print_blocks,
        })
    }
}

impl BlockDevice for LinuxBlockDevice {
    type Error = std::io::Error;

    fn read(
        &self,
        blocks: &mut [Block],
        start_block_idx: BlockIdx,
        reason: &str,
    ) -> Result<(), Self::Error> {
        self.file
            .borrow_mut()
            .seek(SeekFrom::Start(start_block_idx.into_bytes()))?;
        for block in blocks.iter_mut() {
            self.file.borrow_mut().read_exact(&mut block.contents)?;
            if self.print_blocks {
                println!(
                    "Read block ({}) {:?}: {:?}",
                    reason, start_block_idx, &block
                );
            }
        }
        Ok(())
    }

    fn write(&self, blocks: &[Block], start_block_idx: BlockIdx) -> Result<(), Self::Error> {
        self.file
            .borrow_mut()
            .seek(SeekFrom::Start(start_block_idx.into_bytes()))?;
        for block in blocks.iter() {
            self.file.borrow_mut().write_all(&block.contents)?;
            if self.print_blocks {
                println!("Wrote: {:?}", &block);
            }
        }
        Ok(())
    }

    fn num_blocks(&self) -> Result<BlockCount, Self::Error> {
        let num_blocks = self.file.borrow().metadata().unwrap().len() / 512;
        Ok(BlockCount(num_blocks as u32))
    }
}

struct Clock;

impl TimeSource for Clock {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 0,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
        }
    }
}

fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let filename = args.next().unwrap_or_else(|| "/dev/mmcblk0".into());
    let print_blocks = args.find(|x| x == "-v").map(|_| true).unwrap_or(false);
    let lbd = LinuxBlockDevice::new(filename, print_blocks)
        .map_err(Error::DeviceError)
        .unwrap();
    println!("lbd: {:?}", lbd);
    let mut controller: Controller<LinuxBlockDevice, Clock, 4, 4> = Controller::new(lbd, Clock);
    for volume_idx in 0..=3 {
        let volume = controller.get_volume(VolumeIdx(volume_idx));
        println!("volume {}: {:#?}", volume_idx, volume);
        if let Ok(mut volume) = volume {
            let root_dir = controller.open_root_dir(&volume).unwrap();
            println!("\tListing root directory:");
            controller
                .iterate_dir(&volume, &root_dir, |x| {
                    println!("\t\tFound: {:?}", x);
                })
                .unwrap();
            println!("\nCreating file {}...", FILE_TO_DELETE);
            // This will panic if the file already exists, use ReadWriteCreateOrAppend or
            // ReadWriteCreateOrTruncate instead
            let f = controller
                .open_file_in_dir(
                    &mut volume,
                    &root_dir,
                    FILE_TO_DELETE,
                    Mode::ReadWriteCreate,
                )
                .unwrap();

            println!("\tFinding {}...", FILE_TO_DELETE);
            println!(
                "\tFound {}?: {:?}",
                FILE_TO_DELETE,
                controller.find_directory_entry(&volume, &root_dir, FILE_TO_DELETE)
            );

            match controller.delete_file_in_dir(&volume, &root_dir, FILE_TO_DELETE) {
                Ok(()) => (),
                Err(error) => println!("\tCannot delete file: {:?}", error),
            }
            println!("\tClosing {}...", FILE_TO_DELETE);
            controller.close_file(&volume, f).unwrap();

            match controller.delete_file_in_dir(&volume, &root_dir, FILE_TO_DELETE) {
                Ok(()) => println!("\tDeleted {}.", FILE_TO_DELETE),
                Err(error) => println!("\tCannot delete {}: {:?}", FILE_TO_DELETE, error),
            }
            println!("\tFinding {}...", FILE_TO_DELETE);
            println!(
                "\tFound {}?: {:?}",
                FILE_TO_DELETE,
                controller.find_directory_entry(&volume, &root_dir, FILE_TO_DELETE)
            );
        }
    }
}
