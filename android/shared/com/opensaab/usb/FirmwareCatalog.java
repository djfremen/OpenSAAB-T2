// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Metadata only. URLs are the Tech2Wiki Tech2Win download targets, verified 2026-09-10. */
public final class FirmwareCatalog {
    public static final class Entry {
        public final String id,label,url,zipSha,imageSha;
        public final long downloadBytes;
        Entry(String id,String label,long size,String zip,String image){
            this.id=id;this.label=label;downloadBytes=size;zipSha=zip;imageSha=image;
            url="https://raw.githubusercontent.com/tech2wiki/tech2wiki-website/main/assets/binfiles/"+id+".zip";
        }
        public String toString(){return label;}
    }
    public static boolean runnable(Entry e){return !e.id.endsWith("140.500_ru");}
    public static void requireRunnable(String sha)throws java.io.IOException{for(Entry e:ENTRIES)if(e.imageSha.equals(sha)&&!runnable(e))throw new java.io.IOException("Russian 140.500 does not boot with the current emulator profile. Download kept; active card preserved.");}
    public static final Entry[] ENTRIES={
        new Entry("tech2win_card_saab_nao_v9.250_en","Saab NAO · 9.250 · English (default)",2180580,
            "78ac134d2fce4be2ed859da04dcac8a16dc0e16a07f303ab316a9075d54114cc","f5bf579cedc878bb62a8829367fd90c002499fe5fb4cf608b9bba145d174291a"),
        new Entry("tech2win_card_saab_v148.000_en","Saab · 148.000 · English",2180576,
            "62b458902a406e9ed6dd30ca408a4a496b40110293cae01739678ec61a4e6118","e2c4304323d456af3516cea081c4a324448b47f9986328cecf10672ae6c2d4e1"),
        new Entry("tech2win_card_saab_v140.500_ru","Saab · 140.500 · Russian (download only)",2154534,
            "e71b3faad62f3a84b6cedb3cbbc604d33a0e055049f2d43dd31a936509c26e3c","5b80a577eb87434548c784e24aaefb80f4f80f9d6482ea6f37efb2e15a50ca9a")
    };
}
