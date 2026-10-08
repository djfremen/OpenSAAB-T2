# Classic MDI on Android — experimental ARM64 preview

The MDI USB implementation is packaged in the main OpenSAAB app. A separate
helper app or running Windows relay is not required for the tested
Classic MDI fixture. Android USB permission is required. Generic Linux RNDIS
USB0525:a4a2 is a discovery candidate, not proof of exact model or MDI2 support.

1. Install the signed ARM64 preview. Existing installations can upgrade in place.
2. Connect the powered adapter by USB, allow Android USB access, refresh discovery
   and select its USB candidate. The generic discovery name is a tracked UI gap.
3. Preview.64 retrieves the MDI management serial first,
   from a fresh validated adapter announcement, then derives its connection key
   using the classic-MDI protocol material built into the shared core. No saved
   serial or imported connection profile is read. Inventory must identify PDU
   version 2.5.33.154 before registration/START. Firmware selection and download
   follow the normal app setup. Public preview.63 still uses the older profile
   import workflow. The Huawei.6 development build passed uninstall/reinstall with empty app
   storage, normal firmware download, fresh serial, automatic management login,
   fresh VIN, original Main Menu and Stop/USB cleanup on Huawei Android10 ARM64.
   It reinitializes USB configuration2 before claiming the interfaces and joins
   discovery multicast. Pixel regression on this candidate remains pending.
4. Start with VIN and read-only diagnostics. Use original firmware EXIT to leave
   menus and App menu Stop to end the session. Follow physical ignition prompts.
5. Security data can be collected through the original menu or Actions. Existing
   matching collected data can be processed explicitly; stale age alone does not
   block collection. Processing requires internet and the OpenSAAB API; API import
   alone is not an ECU access grant.

On one owner-operated Pixel7 main4 fixture, original SPA Add reached ECU finished
and key status returned Working (Added),4 programmed CIM keys and433MHz. These
are scoped development results. MDI2, other models/vehicles/architectures and full
physical power-loss cold start remain untested.

Known limits: the128MiB structured-recorder size boundary can stop diagnostics;
the ending UI state may lag the transport; the saved DTC exporter may miss list
positions even when the console contains them; host authorization flags do not
recognize every manually completed original-firmware operation. A separate Remove
operation and independently decoded ECU key-grant exchange were not attested.

Raw captures, VINs, seeds/keys and firmware/context backups remain local. Use the
reviewed milestone receipt for public evidence. Matching source contains the app,
OpenMDI core, separate MDI firmware engine and shared dependencies. Native payload
hashes are pinned in the release receipt; independent byte-for-byte rebuilds have
not been established.
