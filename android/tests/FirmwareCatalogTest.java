// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;
public final class FirmwareCatalogTest {
    static void check(boolean ok,String why){if(!ok)throw new AssertionError(why);}
    public static void main(String[] args)throws Exception{
        java.util.Set<String> ids=new java.util.HashSet<>();
        for(String language:FirmwareCatalog.LANGUAGE_CODES){
            FirmwareCatalog.Entry[] entries=FirmwareCatalog.forLanguage(language);
            check(entries.length>0 && FirmwareCatalog.runnable(entries[0]),"No runnable default for "+language);
            for(FirmwareCatalog.Entry e:entries){
                check(ids.add(e.id),"Duplicate card");check(e.zipSha.matches("[a-f0-9]{64}")&&e.imageSha.matches("[a-f0-9]{64}"),"Unpinned firmware");
                check(e.language.equals(language),"Cross-language choice");check(FirmwareCatalog.languageFor(e.id,e.imageSha).equals(language),"Wrong language metadata");
                check(FirmwareCatalog.bySha(e.imageSha)==e,"Ambiguous image");
            }
        }
        check(ids.size()==FirmwareCatalog.ENTRIES.length,"Orphaned firmware");
        check(FirmwareCatalog.forLanguage("jp").length==0,"Unvalidated language exposed");
        FirmwareCatalog.Entry old=FirmwareCatalog.byId("tech2win_card_saab_v140.500_ru");
        try{FirmwareCatalog.requireRunnable(old.imageSha);throw new AssertionError("Known nonbooting card activated");}catch(java.io.IOException expected){}
        System.out.println("FIRMWARE_CATALOG PASS; nine languages with pinned hashes; nonbooting image rejected");
    }
}
