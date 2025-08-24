#include <iostream>
#include <unistd.h>
#include <filesystem>
#include <string>

int main()
{
    std::string BOOT_MODE = " ";
    std::string PART_SUFFIX = " ";
    std::string TARGETDISK = " ";
    std::string HOSTNAME = " ";
    std::string USERNAME = " ";
    std::string PASSWORD = " ";
    char c;

    if (geteuid() != 0)
    {
        std::cout << "This installer must be run as root." << std::endl;
        return 1;
    }

    std::system("clear");

    std::cout << "fluffinstall 0.7 - Fluff Linux Installer\n";
    std::cout << "\n";
    std::cout << "\033[33mNOTE: YOUR SYSTEM WILL REBOOT AUTOMATICALLY WHEN THE INSTALL HAS FINISHED\033[0m\n";
    // This needs to be changed to have it explicitly only accept y or Y and -1 on anything
    std::cout << "Continue? [Y/n]: ";
    std::cin >> c;

    if (c == 'n')
    {
        return 1;
    }

    // Check if the system's firmware is UEFI or LEGACY
    if (std::filesystem::exists("/sys/firmware/efi") && std::filesystem::is_directory("/sys/firmware/efi"))
    {
        BOOT_MODE = "UEFI";
    }
    else
    {
        BOOT_MODE = "BIOS-LEGACY";
    }

    std::cout << "Detected firmware boot mode: " << BOOT_MODE << "\n\n";
    // tell's the user the detected firmware mode'
    std::cout << "Firstly, we want to select our target drive..\n\n\n";

    std::system("lsblk -d --output NAME,MODEL,SIZE,TYPE --noheadings | grep 'disk$'");
    std::cout << "\n\n";

    std::cout << "Enter the name of the target drive you want to install Fluff Linux on. For example: '/dev/sda': /dev/";
    std::cin >> TARGETDISK;

    if (!std::filesystem::exists("/dev/" + TARGETDISK))
    {
        std::cout << TARGETDISK + " is not a valid block device.";
        return 1;
    }

    TARGETDISK = "/dev/" + TARGETDISK;

    if (TARGETDISK.find("nvme") != std::string::npos || TARGETDISK.find("mmcblk") != std::string::npos)
    {
        PART_SUFFIX = "p";
    }
    else
    {
        PART_SUFFIX = "";
    }

    std::cout << "\nYou have selected: " << TARGETDISK << "\n";
    std::cout << "If this isn't the correct drive. Please quit the installer and restart it\n";
    std::cout << "\033[31mTHIS WILL FORMAT THE DRIVE YOU SELECTED AND INSTALL FLUFF LINUX ON IT\033[0m\n";

    std::cout << "Continue? [Y/n]: ";
    std::cin >> c;

    if (c == 'n')
    {
        return 1;
    }


    std::cout << "\nAttempting to format " << TARGETDISK << "\n";

    std::string umountCommand = "umount $(lsblk -nr -o MOUNTPOINT " + TARGETDISK + " | grep -v '^$') 2>/dev/null";
    std::system(umountCommand.c_str());

    std::string wipefsCommand = "wipefs --all " + TARGETDISK;
    std::system(wipefsCommand.c_str());

    if (BOOT_MODE == "UEFI")
    {
        std::system(("parted --script " + TARGETDISK + " mklabel gpt").c_str());
        std::system("sleep 1");
        std::system(("parted --script " + TARGETDISK + " mkpart primary fat32 1MiB 1GiB").c_str());
        std::system(("parted --script " + TARGETDISK + " set 1 esp on").c_str());
        std::system(("parted --script " + TARGETDISK + " name 1 EFI").c_str());
        std::system(("parted --script " + TARGETDISK + " mkpart primary linux-swap 1GiB 5GiB").c_str());
        std::system(("parted --script " + TARGETDISK + " name 2 SWAP").c_str());
        std::system(("parted --script " + TARGETDISK + " mkpart primary ext4 5GiB 100%").c_str());
        std::system(("parted --script " + TARGETDISK + " name 3 FluffLinux").c_str());
    }
    else
    {
        std::system(("parted --script " + TARGETDISK + " mklabel msdos").c_str());
        std::system(("parted --script " + TARGETDISK + " mkpart primary linux-swap 1MiB 5GiB").c_str());
        std::system(("parted --script " + TARGETDISK + " mkpart primary ext4 5GiB 100%").c_str());
    }

    std::string BOOT_PART = TARGETDISK + PART_SUFFIX + "1";
    std::string SWAP_PART, ROOT_PART;

    if (BOOT_MODE == "UEFI")
    {
        SWAP_PART = TARGETDISK + PART_SUFFIX + "2";
        ROOT_PART = TARGETDISK + PART_SUFFIX + "3";
    }
    else
    {
        SWAP_PART = TARGETDISK + PART_SUFFIX + "1";
        ROOT_PART = TARGETDISK + PART_SUFFIX + "2";
    }

    if (BOOT_MODE == "UEFI")
        std::system(("mkfs.fat -F32 -n EFI " + BOOT_PART).c_str());
    std::system(("mkswap -L SWAP " + SWAP_PART).c_str());
    std::system(("mkfs.ext4 -F -L FluffLinux " + ROOT_PART).c_str());

    std::system(("mount " + ROOT_PART + " /mnt").c_str());
    if (BOOT_MODE == "UEFI")
        std::system(("mount --mkdir " + BOOT_PART + " /mnt/boot").c_str());
    std::system(("swapon " + SWAP_PART).c_str());

    std::cout << "\nInstalling system...\n";
    std::system("pacstrap -C /etc/pacman.d/fluffinstall.conf -K /mnt base flufflinux-filesystem linux linux-atm linux-firmware linux-firmware-marvell broadcom-wl linux-firmware-bnx2x amd-ucode arch-install-scripts intel-ucode b43-fwcutter bolt clonezilla cryptsetup ddrescue diffutils dmidecode dmraid dnsmasq dosfstools e2fsprogs edk2-shell efibootmgr grub ethtool exfatprogs fatresize fsarchiver gpart git gpm gptfdisk hdparm less libusb-compat livecd-sounds lsscsi lvm2 man-db man-pages mdadm memtest86+ memtest86+-efi mkinitcpio mkinitcpio-archiso mkinitcpio-nfs-utils modemmanager mtools nano nfs-utils nmap ntfs-3g nvme-cli open-iscsi openssh partclone parted  networkmanager networkmanager-openvpn partimage pv qemu-guest-agent rp-pppoe rsync sdparm sg3_utils smartmontools squashfs-tools sudo syslinux systemd-resolvconf tcpdump testdisk tmux tpm2-tools tpm2-tss udftools usb_modeswitch usbmuxd usbutils vim virtualbox-guest-utils-nox wireless-regdb wpa_supplicant wvdial xfsprogs zsh grml-zsh-config fastfetch htop konsole kate dolphin kdialog alsa-lib alsa-utils alsa-ucm-conf pipewire pipewire-pulse wireplumber pipewire-alsa pipewire-jack sof-firmware sddm mesa vulkan-intel vulkan-mesa-layers vulkan-tools nvidia nvidia-utils vulkan-radeon vulkan-icd-loader system-config-printer cups firefox gnome-disk-utility noto-fonts noto-fonts-cjk noto-fonts-emoji noto-fonts-extra ttf-liberation flatpak gnome-calculator vlc ffmpegthumbs kdegraphics-thumbnailers thunderbird libreoffice-still gwenview qt5-imageformats spectacle speech-dispatcher lib32-alsa-lib lib32-alsa-plugins lib32-libpulse lib32-pipewire lib32-alsa-oss lib32-mesa lib32-vulkan-radeon lib32-vulkan-intel lib32-nvidia-utils lib32-sdl2 qemu-full libvirt tlp tlp-rdw thermald libimobiledevice ifuse gvfs-mtp android-udev gvfs-gphoto2 gphoto2 hplip base-devel yay btop traceroute ark remmina freerdp libvncserver edk2-ovmf vlc-plugin-gstreamer vlc-plugin-ffmpeg aurorae bluedevil breeze breeze-gtk breeze-plymouth discover drkonqi flatpak-kcm kactivitymanagerd kde-cli-tools kde-gtk-config kdecoration kdeplasma-addons kgamma kglobalacceld kinfocenter kmenuedit kpipewire krdp kscreen kscreenlocker ksshaskpass ksystemstats kwallet-pam kwayland kwin kwin-x11 kwrited layer-shell-qt libkscreen libksysguard libplasma milou ocean-sound-theme oxygen oxygen-sounds plasma-activities plasma-activities-stats plasma-browser-integration plasma-desktop plasma-disks plasma-firewall plasma-integration plasma-nm plasma-pa plasma-sdk plasma-systemmonitor plasma-thunderbolt plasma-vault plasma-welcome plasma-workspace plasma-workspace-wallpapers plasma5support plymouth-kcm polkit-kde-agent powerdevil print-manager qqc2-breeze-style sddm-kcm spectacle systemsettings wacomtablet xdg-desktop-portal-kde");

    // Copy a bunch of custom files into the filesystem
    std::system("cp /etc/os-release /mnt/etc/");
    std::system("cp /usr/lib/os-release /mnt/usr/lib/");
    std::system("cp /etc/motd /mnt/etc/");
    std::system("cp /etc/issue /mnt/etc/");

    std::system("mkdir /mnt/etc/skel/.local");
    std::system("mkdir /mnt/etc/skel/.local/state");
    std::system("mkdir /mnt/etc/skel/.local/share");
    std::system("mkdir /mnt/etc/skel/.local/share/konsole");

    std::system("cp /etc/skel/.local/state/dolphinstaterc /mnt/etc/skel/.local/state/");
    std::system("cp /etc/skel/.local/share/konsole/* /mnt/etc/skel/.local/share/konsole/");
    std::system("cp -r /etc/skel/.config/ /mnt/etc/skel/");
    std::system("cp /etc/nanorc /mnt/etc/");

    // Generate Fstab file
    std::cout << "Generating Fstab... \n";
    std::system("genfstab -U /mnt >> /mnt/etc/fstab");

    // Start Configuring the system (Hostname,Username,password)
    std::cout << "\nConfiguring system... \n\n";

    std::cout << "Enter the hostname/system name you'd like to have: ";
    std::cin >> HOSTNAME;

    std::cout << "Enter the user name you'd like to have: ";
    std::cin >> USERNAME;

    std::cout << "Write down the password you'd like to have for " << USERNAME << ": ";
    std::cin >> PASSWORD;

    std::string archChrootCmd =
    "arch-chroot /mnt /bin/bash -c '"
    "echo \"" + HOSTNAME + "\" > /etc/hostname && "
    "useradd -m -G wheel,kvm,libvirt -s /bin/zsh " + USERNAME + " && "
    "echo \"" + USERNAME + ":" + PASSWORD + "\" | chpasswd && "
    "sed -i \"s/^# %wheel ALL=(ALL:ALL) ALL/%wheel ALL=(ALL:ALL) ALL/\" /etc/sudoers'";


    std::system(archChrootCmd.c_str());

    std::cout << "Configuring BootLoader (GRUB) ... \n";
    if (BOOT_MODE == "UEFI")
    {
        std::system("arch-chroot /mnt grub-install --target=x86_64-efi --efi-directory=/boot --removable --boot-directory=/boot");
    }
    else
    {
        std::system(("arch-chroot /mnt grub-install --target=i386-pc --recheck " + TARGETDISK + " --boot-directory=/boot").c_str());
    }
    std::system("cp /etc/default/grub /mnt/etc/default/grub");
    std::system("cp /etc/grub.d/10_linux /mnt/etc/grub.d/10_linux");
    std::system("arch-chroot /mnt grub-mkconfig -o /boot/grub/grub.cfg");

    // bootloader setup finished.

    // copy more system files here:
    std::system("cp /etc/pacman.conf /mnt/etc/");
    std::system("cp /etc/pacman.d/mirrorlist /mnt/etc/pacman.d/mirrorlist");
    std::system("cp /etc/locale.conf /mnt/etc/");
    std::system("mkdir /mnt/etc/sddm.conf.d");
    std::system("cp /etc/fonts/conf.d/99-emoji-fallback.conf /mnt/etc/fonts/conf.d/");
    std::system("cp /etc/skel/.sddm.conf.d/kde_settings.conf /mnt/etc/sddm.conf.d");
    std::system("cp -r /usr/share/sddm/themes/fluff-breeze/ /mnt/usr/share/sddm/themes/");
    std::system("cp /usr/share/pixmaps/* /mnt/usr/share/pixmaps/");
    std::system("ln -sf /usr/share/zoneinfo/UTC /mnt/etc/localtime"); //keep this until timezone selection is implemented
    std::system("arch-chroot /mnt fc-cache -fv");
    std::system("cp /usr/lib/firefox/distribution/policies.json /mnt/usr/lib/firefox/distribution/"); //middle mouse scroll

    // getwine stuff
    std::system("cp -r /etc/getwine /mnt/etc/");
    std::system("cp /usr/bin/getwine /mnt/usr/bin");
    std::system("arch-chroot /mnt chmod +x /usr/bin/getwine");
    std::system("cp /etc/getwine/getwine.desktop /mnt/usr/share/applications");
    std::system("arch-chroot /mnt chmod +x /usr/share/applications/getwine.desktop");


    //enable system services
    std::system("arch-chroot /mnt systemctl enable sddm");
    std::system("arch-chroot /mnt systemctl enable NetworkManager");
    std::system("arch-chroot /mnt ln -sf /run/NetworkManager/resolv.conf /etc/resolv.conf");
    std::system("arch-chroot /mnt systemctl enable bluetooth");
    std::system("arch-chroot /mnt systemctl enable libvirtd");
    std::system("arch-chroot /mnt systemctl enable tlp");
    std::system("arch-chroot /mnt systemctl enable fstrim.timer");
    std::system("arch-chroot /mnt systemctl enable cups");
    std::system("arch-chroot /mnt systemctl enable avahi-daemon");
    std::system("arch-chroot /mnt systemctl enable thermald.service");

    // virtmanager flatpak perms/OOBE
    std::system(("arch-chroot /mnt setfacl -m u:libvirt-qemu:rwx /home/" + USERNAME).c_str());
    std::system("arch-chroot /mnt flatpak override --filesystem=home org.virt_manager.virt-manager");


    std::cout << "\033[32mThe installation has finished! Your system will reboot automatically in 3 seconds...\033[0m\n";
    std::system("sleep 3");
    std::system("reboot now");

    return 0;
}

