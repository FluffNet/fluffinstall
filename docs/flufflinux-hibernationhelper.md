# Fluff Linux Hibernation Helper

## Purpose

`flufflinux-hibernationhelper` will provide dynamic hibernation storage without
requiring Fluff Linux to reserve a RAM-sized swap partition or swapfile during
installation.

The permanent swap area and the temporary hibernation area will be separate:

```text
/swap/
|- swapfile          Permanent 4 GiB swapfile
`- hibernate.swap    Temporary file created only for hibernation
```

Both files will live in a dedicated Btrfs subvolume. On an encrypted system,
the files will therefore be protected by the root LUKS container without
requiring a separate encrypted swap device.

## Current installer foundation

FluffInstall creates:

- A 1 GiB EFI system partition.
- One Btrfs root partition using the remainder of the drive.
- A `/swap` Btrfs subvolume.
- A permanent 4 GiB `/swap/swapfile` created with
  `btrfs filesystem mkswapfile`.
- An `/etc/fstab` entry that activates the permanent swapfile at boot.

The permanent file replaces the previous 4 GiB swap partition. Its initial
size remains 4 GiB, but a future System Settings page can resize it without
repartitioning the drive.

The `/swap` subvolume must not be included in future system snapshots.

## Planned package contents

The helper package should contain:

- A privileged Rust preparation and cleanup helper.
- A systemd service ordered before `systemd-hibernate.service`.
- A dependency that prevents `systemd-hibernate.service` from running when
  preparation fails.
- A cleanup service for successful resume, failed resume, normal boot and
  interrupted preparation.
- A systemd-logind drop-in that allows KDE to offer hibernation before the
  temporary swapfile exists.
- Initramfs integration for systemd's `HibernateLocation` EFI variable.
- Clear user-facing error reporting.

The implementation should use systemd drop-ins and documented kernel and
Btrfs interfaces. It should not patch KDE or overwrite package-owned systemd
unit files.

## Hibernation preparation

When the user requests hibernation, the helper will:

1. Remove any safe, stale temporary hibernation file.
2. Read current hibernation-relevant memory and swap use.
3. Add a fixed 1 GiB safety allowance.
4. Round the result upward to an allocation boundary.
5. Confirm that Btrfs can allocate the requested space.
6. Create `/swap/hibernate.swap` with
   `btrfs filesystem mkswapfile`.
7. Activate it at a higher swap priority than `/swap/swapfile`.
8. Verify its physical resume offset with
   `btrfs inspect-internal map-swapfile`.
9. Allow `systemd-hibernate.service` to continue only after every check
   succeeds.

The initial sizing inputs should include anonymous memory, unevictable memory,
existing swap use and required kernel overhead. Clean filesystem cache does not
normally need to be stored in the hibernation image.

The temporary file must be fully allocated before hibernation. Btrfs and the
kernel cannot expand it while the memory image is being written.

## Failure behavior

If allocation, activation or validation fails:

- Hibernation is cancelled.
- The computer remains awake.
- Any partially prepared temporary file is removed safely.
- The user receives a clear error explaining the failure.

The system must never power off after a failed preparation.

## Resume and cleanup

On UEFI systems, systemd can record the selected device and swapfile offset in
the `HibernateLocation` EFI variable. A systemd-based initramfs can read that
location before mounting the root filesystem and restore the image.

After a successful resume, the helper will deactivate and delete
`/swap/hibernate.swap`. An early boot cleanup path will also remove a stale file
after a normal boot or failed resume, but only after the initramfs has attempted
resume.

The temporary file consumes disk space only while a hibernation operation is
prepared or the computer is hibernated.

## Btrfs requirements

The swapfiles must be:

- Fully preallocated.
- NODATACOW.
- Uncompressed.
- Free of holes.
- Stored on a supported single-device Btrfs data profile.

`btrfs filesystem mkswapfile` should be used instead of manually reproducing
these requirements.

An active Btrfs swapfile prevents snapshots of its containing subvolume and can
restrict some balance, device and filesystem maintenance operations. Keeping
swap in its own subvolume contains the snapshot restriction and makes future
management clearer.

## Mechanical drives

The layout is supported on both solid-state and mechanical drives. Mechanical
drives will have slower swap access, hibernation and resume times, but require
no different on-disk layout or installer behavior.

## Future System Settings integration

A future settings module may allow the user to:

- Change the permanent swapfile size.
- Disable permanent swap.
- View whether hibernation is currently available.
- View estimated temporary hibernation storage requirements.
- Test the resume configuration.

Resizing must use `swapoff`, recreate the file safely, reactivate it and retain
the correct `/etc/fstab` entry. Hibernation preparation and permanent swap
resizing must never run concurrently.
