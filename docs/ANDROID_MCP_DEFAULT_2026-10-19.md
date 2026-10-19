# Foreground MCP and Bluetooth candidate wording

OpenSAAB **0.1.1-preview.7 ARM64**, version code 101007, changes Android's
MCP default and Bluetooth selection wording. It retains preview.6's six pinned
legacy, adapter and connection workers. It does not change vehicle commands or
add adapter capabilities.

MCP starts automatically when the foreground workspace binds its owner. Start
the local bridge with the app's pairing code once, using a private mode-0600
token file, an explicit paired ADB serial and one persistent stdio process:

```text
python3 opensaab-android-mcp.py --serial PAIRED_ADB_SERIAL --token-file /private/pairing-code.txt
```

The app keeps its random pairing credential in private no-backup storage across
background/foreground changes and process restarts. Backgrounding still closes
the endpoint, clients and observers; returning to the foreground resumes it.
**AI control (MCP) → Disable** persists until explicitly re-enabled and deletes
the pairing credential. Re-enabling creates a new code. Authentication, session,
menu, consumed-input and manual-takeover checks remain required. No adapter or
diagnostic session starts merely because MCP is available. Physical ignition
actions and diagnostic/security consent remain operator actions.

The connection chooser now says **Bluetooth adapters**. Matching paired or
nearby names appear as **candidates**, and model/firmware checks still follow
explicit selection. Discovery and pairing do not establish compatibility.
Existing Carista EVO and vLinker routes retain their limited legacy scope.
The owner's current Fire Bluetooth validation uses **OBDLink MX+ only**;
reported Android successes with other models do not qualify this APK, the Fire,
or every diagnostic workflow.

Preview.6's MDI run observed startup VIN, counter reset and zero readback,
the DTC EXIT route and a working ignition key. Three control response timeouts,
partial menu redraws and a one-entry versus earlier sixteen-position code list
remain under investigation. This change does not fix or reclassify them. Keep
exact APK/source, installed lifecycle/UI evidence and adapter/vehicle receipts
separate. ARM32 source adoption, other-platform migration and complete physical
qualification remain pending. This candidate is unpublished.
