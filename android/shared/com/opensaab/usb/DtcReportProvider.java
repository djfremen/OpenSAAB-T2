// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

import android.content.*;
import android.database.*;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.*;

/** Grants the chosen share recipient read access to one report, never session logs. */
public final class DtcReportProvider extends ContentProvider {
    public boolean onCreate(){return true;}
    private File file(Uri uri) throws FileNotFoundException {
        if(!(getContext().getPackageName()+".dtc-reports").equals(uri.getAuthority()) || uri.getPathSegments().size()!=1)
            throw new FileNotFoundException("Not a DTC report");
        String name=uri.getLastPathSegment();
        if(!name.matches("android_(chipsoft|nano)_dtc_[0-9T-Z-]+_[a-f0-9-]+\\.(txt|json)"))throw new FileNotFoundException("Not a DTC report");
        File parent=new File(getContext().getFilesDir(),"dtc-reports"), f=new File(parent,name);
        try{if(!f.getCanonicalFile().getParentFile().equals(parent.getCanonicalFile()) || !f.isFile())throw new FileNotFoundException();}
        catch(IOException e){throw new FileNotFoundException("Report unavailable");}
        return f;
    }
    public ParcelFileDescriptor openFile(Uri uri,String mode) throws FileNotFoundException {
        if(!"r".equals(mode))throw new FileNotFoundException("Read only");
        return ParcelFileDescriptor.open(file(uri),ParcelFileDescriptor.MODE_READ_ONLY);
    }
    public String getType(Uri uri){return uri.getPath()!=null && uri.getPath().endsWith(".json")?"application/json":"text/plain";}
    public Cursor query(Uri uri,String[] projection,String selection,String[] args,String sort){
        try{
            File f=file(uri);String[] cols=projection==null?new String[]{OpenableColumns.DISPLAY_NAME,OpenableColumns.SIZE}:projection;
            MatrixCursor cursor=new MatrixCursor(cols);Object[] row=new Object[cols.length];
            for(int i=0;i<cols.length;i++)row[i]=OpenableColumns.DISPLAY_NAME.equals(cols[i])?f.getName():OpenableColumns.SIZE.equals(cols[i])?f.length():null;
            cursor.addRow(row);return cursor;
        }catch(FileNotFoundException e){return null;}
    }
    public Uri insert(Uri u,ContentValues v){throw new UnsupportedOperationException("Read only");}
    public int update(Uri u,ContentValues v,String s,String[] a){throw new UnsupportedOperationException("Read only");}
    public int delete(Uri u,String s,String[] a){throw new UnsupportedOperationException("Read only");}
}
