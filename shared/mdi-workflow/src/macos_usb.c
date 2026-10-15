// SPDX-License-Identifier: MPL-2.0
// IOKit ownership/I/O only. Shared Rust owns configuration profile and protocol.
#include <IOKit/IOCFPlugIn.h>
#include <IOKit/usb/IOUSBLib.h>
#include <CoreFoundation/CoreFoundation.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>

typedef struct {
    IOUSBDeviceInterface500 **device;
    IOUSBInterfaceInterface550 **control, **data;
    CFRunLoopSourceRef source;
    UInt8 input, output;
    unsigned char buffer[2048];
    int pending, completed, reading;
    IOReturn status;
    UInt32 length;
} MdiUSB;

static int number(io_service_t entry, const char *key, uint32_t *out) {
    CFStringRef name=CFStringCreateWithCString(NULL,key,kCFStringEncodingASCII);
    CFTypeRef value=IORegistryEntryCreateCFProperty(entry,name,NULL,0);
    CFRelease(name);
    int ok=value && CFGetTypeID(value)==CFNumberGetTypeID() &&
        CFNumberGetValue((CFNumberRef)value,kCFNumberSInt32Type,out);
    if(value) CFRelease(value);
    return ok;
}
static void completed(void *refcon, IOReturn status, void *count) {
    MdiUSB *usb=refcon;
    usb->status=status; usb->length=(UInt32)(uintptr_t)count;
    usb->pending=0; usb->completed=1;
}
static IOReturn submit(MdiUSB *usb) {
    usb->pending=1; usb->completed=0; usb->length=0;
    IOReturn rc=(*usb->data)->ReadPipeAsync(usb->data,usb->input,usb->buffer,sizeof(usb->buffer),completed,usb);
    if(rc) usb->pending=0;
    return rc;
}
int mdi_usb_stop(MdiUSB *usb, unsigned char *bytes, uint32_t *length, int *status) {
    usb->reading=0;
    if(usb->pending) {
        IOReturn rc=(*usb->data)->AbortPipe(usb->data,usb->input);
        if(rc) return rc;
        CFAbsoluteTime deadline=CFAbsoluteTimeGetCurrent()+2;
        while(usb->pending && CFAbsoluteTimeGetCurrent()<deadline)
            CFRunLoopRunInMode(kCFRunLoopDefaultMode,0.01,true);
        if(usb->pending) return kIOReturnTimeout;
    }
    *length=0; *status=0;
    if(usb->completed) {
        if(usb->length>sizeof(usb->buffer)) return kIOReturnOverrun;
        *length=usb->length; *status=usb->status;
        memcpy(bytes,usb->buffer,*length);usb->completed=0;
    }
    return 0;
}
int mdi_usb_close(MdiUSB *usb) {
    if(!usb) return 0;
    if(usb->pending) {
        unsigned char b[2048];uint32_t n;int status;
        int rc=mdi_usb_stop(usb,b,&n,&status);
        // A pending callback retains its memory/handles until process exit.
        if(rc) return rc;
    }
    IOReturn result=0,rc;
    if(usb->source) {
        CFRunLoopRemoveSource(CFRunLoopGetCurrent(),usb->source,kCFRunLoopDefaultMode);
        CFRelease(usb->source);
    }
    if(usb->data) {rc=(*usb->data)->USBInterfaceClose(usb->data);if(rc)result=rc;(*usb->data)->Release(usb->data);}
    if(usb->control) {rc=(*usb->control)->USBInterfaceClose(usb->control);if(rc)result=rc;(*usb->control)->Release(usb->control);}
    if(usb->device) {rc=(*usb->device)->USBDeviceClose(usb->device);if(rc)result=rc;(*usb->device)->Release(usb->device);}
    free(usb);return result;
}
void *mdi_usb_open(uint32_t location, uint64_t identity, int *error) {
    io_iterator_t devices=0;io_service_t found=0,entry;
    *error=IOServiceGetMatchingServices(kIOMainPortDefault,IOServiceMatching("IOUSBHostDevice"),&devices);
    if(*error) return NULL;
    while((entry=IOIteratorNext(devices))) {
        uint32_t vendor=0,product=0,at=0;uint64_t registry=0;
        IORegistryEntryGetRegistryEntryID(entry,&registry);
        if(number(entry,"idVendor",&vendor) && number(entry,"idProduct",&product) &&
           number(entry,"locationID",&at) && vendor==0x0525 && product==0xa4a2 &&
           at==location && registry==identity) {found=entry;break;}
        IOObjectRelease(entry);
    }
    IOObjectRelease(devices);
    if(!found){*error=kIOReturnNoDevice;return NULL;}
    MdiUSB *usb=calloc(1,sizeof(*usb));IOCFPlugInInterface **plugin=NULL;SInt32 score=0;
    *error=IOCreatePlugInInterfaceForService(found,kIOUSBDeviceUserClientTypeID,kIOCFPlugInInterfaceID,&plugin,&score);
    IOObjectRelease(found);
    if(*error || !plugin){free(usb);return NULL;}
    *error=(*plugin)->QueryInterface(plugin,CFUUIDGetUUIDBytes(kIOUSBDeviceInterfaceID500),(LPVOID*)&usb->device);
    (*plugin)->Release(plugin);
    if(*error || !usb->device){free(usb);return NULL;}
    *error=(*usb->device)->USBDeviceOpen(usb->device);
    if(*error){(*usb->device)->Release(usb->device);free(usb);return NULL;}
    // Same observed classic-MDI config2 reset as Android, before protocol I/O.
    *error=(*usb->device)->SetConfiguration(usb->device,2);
    UInt8 configured=0;
    if(!*error)*error=(*usb->device)->GetConfiguration(usb->device,&configured);
    if(!*error && configured!=2)*error=kIOReturnUnsupported;
    io_iterator_t interfaces=0;
    IOUSBFindInterfaceRequest request={kIOUSBFindInterfaceDontCare,kIOUSBFindInterfaceDontCare,kIOUSBFindInterfaceDontCare,kIOUSBFindInterfaceDontCare};
    if(!*error)*error=(*usb->device)->CreateInterfaceIterator(usb->device,&request,&interfaces);
    while(!*error && (entry=IOIteratorNext(interfaces))) {
        IOUSBInterfaceInterface550 **interface=NULL;plugin=NULL;
        *error=IOCreatePlugInInterfaceForService(entry,kIOUSBInterfaceUserClientTypeID,kIOCFPlugInInterfaceID,&plugin,&score);
        IOObjectRelease(entry);
        if(*error)break;
        *error=(*plugin)->QueryInterface(plugin,CFUUIDGetUUIDBytes(kIOUSBInterfaceInterfaceID550),(LPVOID*)&interface);
        (*plugin)->Release(plugin);
        if(*error)break;
        UInt8 number=255;*error=(*interface)->GetInterfaceNumber(interface,&number);
        if(*error || number>1){(*interface)->Release(interface);if(*error)break;continue;}
        *error=(*interface)->USBInterfaceOpen(interface);
        if(*error){(*interface)->Release(interface);break;}
        if(number==0)usb->control=interface;else usb->data=interface;
    }
    if(interfaces)IOObjectRelease(interfaces);
    if(!*error && (!usb->control || !usb->data))*error=kIOReturnUnsupported;
    UInt8 endpoints=0;
    if(!*error)*error=(*usb->data)->GetNumEndpoints(usb->data,&endpoints);
    for(UInt8 pipe=1; !*error && pipe<=endpoints;pipe++) {
        UInt8 direction,number,type,interval;UInt16 maximum;
        *error=(*usb->data)->GetPipeProperties(usb->data,pipe,&direction,&number,&type,&maximum,&interval);
        if(!*error && type==kUSBBulk && maximum==64) {
            if(direction==kUSBIn && number==1)usb->input=pipe;
            if(direction==kUSBOut && number==2)usb->output=pipe;
        }
    }
    if(!*error && (!usb->input || !usb->output))*error=kIOReturnUnsupported;
    if(!*error)*error=(*usb->data)->CreateInterfaceAsyncEventSource(usb->data,&usb->source);
    if(*error){mdi_usb_close(usb);return NULL;}
    CFRunLoopAddSource(CFRunLoopGetCurrent(),usb->source,kCFRunLoopDefaultMode);
    return usb;
}
int mdi_usb_control(MdiUSB *usb,uint8_t kind,uint8_t request,unsigned char *bytes,uint16_t length,uint32_t *actual) {
    IOUSBDevRequestTO req={0};req.bmRequestType=kind;req.bRequest=request;
    req.wLength=length;req.pData=bytes;req.noDataTimeout=1000;req.completionTimeout=1000;
    IOReturn rc=(*usb->device)->DeviceRequestTO(usb->device,&req);*actual=req.wLenDone;return rc;
}
int mdi_usb_write(MdiUSB *usb,unsigned char *bytes,uint32_t length) {
    return (*usb->data)->WritePipeTO(usb->data,usb->output,bytes,length,1000,1000);
}
int mdi_usb_start(MdiUSB *usb){usb->reading=1;return submit(usb);}
int mdi_usb_poll(MdiUSB *usb,unsigned char *bytes,uint32_t *length) {
    *length=0;CFRunLoopRunInMode(kCFRunLoopDefaultMode,0.0001,true);
    if(!usb->completed)return 0;
    IOReturn rc=usb->status;
    if(rc)return rc;
    if(usb->length>sizeof(usb->buffer))return kIOReturnOverrun;
    *length=usb->length;memcpy(bytes,usb->buffer,*length);usb->completed=0;
    if(usb->reading)rc=submit(usb);
    return rc ? rc : 1;
}
