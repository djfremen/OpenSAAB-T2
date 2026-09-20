// SPDX-License-Identifier: MPL-2.0
package com.opensaab.usb;

/** Metadata only. URLs are the Tech2Wiki Tech2/Tech2Win download targets, verified 2026-09-10. */
public final class FirmwareCatalog {
    public static final class Entry {
        public final String id,label,url,zipSha,imageSha,language;
        public final long downloadBytes;
        Entry(String id,String label,long size,String zip,String image){
            this.id=id;this.label=label;downloadBytes=size;zipSha=zip;imageSha=image;language=id.substring(id.lastIndexOf('_')+1);
            url="https://raw.githubusercontent.com/tech2wiki/tech2wiki-website/main/assets/binfiles/"+id+".zip";
        }
        public String toString(){return label;}
    }
    public static final String[] LANGUAGE_CODES={"en","de","es","fi","fr","it","nl","ru","se"};
    public static final String[] LANGUAGE_NAMES={"English","Deutsch","Español","Suomi","Français","Italiano","Nederlands","Русский","Svenska"};
    public static int languageIndex(String code){for(int i=0;i<LANGUAGE_CODES.length;i++)if(LANGUAGE_CODES[i].equals(code))return i;return 0;}
    public static Entry[] forLanguage(String code){java.util.List<Entry> found=new java.util.ArrayList<>();for(Entry e:ENTRIES)if(e.language.equals(code))found.add(e);return found.toArray(new Entry[0]);}
    public static Entry byId(String id){for(Entry e:ENTRIES)if(e.id.equals(id))return e;return null;}
    public static Entry bySha(String sha){for(Entry e:ENTRIES)if(e.imageSha.equals(sha))return e;return null;}
    public static String languageFor(String id,String sha){Entry e=byId(id);if(e==null)e=bySha(sha);return e==null?"unknown":e.language;}
    public static boolean runnable(Entry e){return !e.id.endsWith("140.500_ru");}
    public static void requireRunnable(String sha)throws java.io.IOException{for(Entry e:ENTRIES)if(e.imageSha.equals(sha)&&!runnable(e))throw new java.io.IOException("Russian 140.500 does not boot with the current emulator profile. Download kept; active card preserved.");}
    public static final Entry[] ENTRIES={
        new Entry("tech2win_card_saab_nao_v9.250_en","Saab NAO · 9.250 · English (default)",2180580,
            "78ac134d2fce4be2ed859da04dcac8a16dc0e16a07f303ab316a9075d54114cc","f5bf579cedc878bb62a8829367fd90c002499fe5fb4cf608b9bba145d174291a"),
        new Entry("tech2win_card_saab_v148.000_en","Saab · 148.000 · English",2180576,
            "62b458902a406e9ed6dd30ca408a4a496b40110293cae01739678ec61a4e6118","e2c4304323d456af3516cea081c4a324448b47f9986328cecf10672ae6c2d4e1"),
        new Entry("tech2_card_saab_v148.000_de","Saab · 148.000 · Deutsch",2421960,
            "92f9b275a1801f4ea333e0064f9b7868906d1d58155d9b031d980f0927745cc4","ffd34179ddbfcb96580a83dd1b4f02cce68519f4b1f9d75ebedadbd8d40019cd"),
        new Entry("tech2_card_saab_v148.000_es","Saab · 148.000 · Español",2470773,
            "aecdc0969eb2151dc054776208ce2b683504cc8ec8679c434266f44fb3a22553","b636a46227b53c32b8812521d76af97f9af01044ad6be0d165e1b7e0938dc7c5"),
        new Entry("tech2_card_saab_v148.000_fi","Saab · 148.000 · Suomi",2436613,
            "140fd50c04c1dfa6d594ccbe8d7eb669ee048729db58c781718ff7c185ccf689","945c52147639676389911a3c66b98b4da302693ebc2db421d79c87452b53108f"),
        new Entry("tech2_card_saab_v148.000_fr","Saab · 148.000 · Français",2467231,
            "fb7a3cbda93782ad380df5f92ec40d7b4d303bf1a13a156a8a46d5a00f04caad","dea8abb0c25525577c93de7207545a9c463666859b35b8f44c47d458cd5dcefe"),
        new Entry("tech2_card_saab_v148.000_it","Saab · 148.000 · Italiano",2440417,
            "e7acb807ef8e91c15ad3149116e5b2f6a9b96096fe1de55578271992c3a4860a","963b8eacc01a69b9da3e86dc4f520114202b25579a3d507fc489f345592ecf8e"),
        new Entry("tech2_card_saab_v148.000_nl","Saab · 148.000 · Nederlands",2438545,
            "b4a2c76100dfaa835dc4d333f35f5ebc8a8ac4e09b52274ef227e3ec1ebaf903","add31f4f216871f413ab11c8802e4c95d7911011d567ae0d076702696826ef05"),
        new Entry("tech2_card_saab_v148.000_ru","Saab · 148.000 · Русский",2487961,
            "62df666677a08e0fe5d3313ccaddced25b9c0bf47f0a69adfedafb494a4fad6e","abeed479be92ccf86d30d177116c4ebef7303c9315fa8d31c566d58a733dcf0a"),
        new Entry("tech2_card_saab_v148.000_se","Saab · 148.000 · Svenska",2425091,
            "9122100602493f59b9f107e4f380f314445b8dc97fcd48beb7669d9dca924a54","3af7891734a44a614ffdce2069756cf1ce9add3df13e65c903654fb38c684908"),
        new Entry("tech2win_card_saab_v140.500_ru","Saab · 140.500 · Russian (download only)",2154534,
            "e71b3faad62f3a84b6cedb3cbbc604d33a0e055049f2d43dd31a936509c26e3c","5b80a577eb87434548c784e24aaefb80f4f80f9d6482ea6f37efb2e15a50ca9a")
    };
}
