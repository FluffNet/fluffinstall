# fluffinstall

fluffinstall is the installer for Fluff Linux.

---

## Building

To build the installer:

1. Make the build script executable:
   ```bash
   chmod +x build.sh
   ```

2. Run the build script:
   ```bash
   ./build.sh
   ```

This will compile `fluffinstall.cpp` into an executable named **`fluffinstall`**.
Use inside the Fluff Linux live session.

---

## Notes

- Ensure you have a C++ compiler installed (`Preferably g++ as that is used by the build.sh script`)
- Make sure you run the installer as root
