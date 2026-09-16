// SPDX-License-Identifier: MPL-2.0
package com.opensaab.checker;
import android.content.*;
import android.database.*;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.*;
/** Only a single verified installer filename may be granted to Android's installer. */
public final class InstallerProvider extends ContentProvider {
    public boolean onCreate(){return true;}
    private File file(Uri u)throws FileNotFoundException{
        if(!"com.opensaab.checker.installers".equals(u.getAuthority())||u.getPathSegments().size()!=1||!u.getLastPathSegment().matches("[a-f0-9]{64}\\.apk"))throw new FileNotFoundException();
        return new File(new File(getContext().getFilesDir(),"installers"),u.getLastPathSegment());
    }
    public ParcelFileDescriptor openFile(Uri u,String mode)throws FileNotFoundException{if(!"r".equals(mode))throw new FileNotFoundException();return ParcelFileDescriptor.open(file(u),ParcelFileDescriptor.MODE_READ_ONLY);}
    public String getType(Uri u){return "application/vnd.android.package-archive";}
    public Cursor query(Uri u,String[] projection,String selection,String[] args,String sort){
        try{File f=file(u);String[] cols=projection==null?new String[]{OpenableColumns.DISPLAY_NAME,OpenableColumns.SIZE}:projection;MatrixCursor c=new MatrixCursor(cols);Object[] row=new Object[cols.length];for(int i=0;i<cols.length;i++)row[i]=OpenableColumns.DISPLAY_NAME.equals(cols[i])?"OpenSAAB.apk":OpenableColumns.SIZE.equals(cols[i])?f.length():null;c.addRow(row);return c;}catch(Exception e){return null;}
    }
    public Uri insert(Uri u,ContentValues v){throw new UnsupportedOperationException();}
    public int update(Uri u,ContentValues v,String s,String[] a){throw new UnsupportedOperationException();}
    public int delete(Uri u,String s,String[] a){throw new UnsupportedOperationException();}
}
