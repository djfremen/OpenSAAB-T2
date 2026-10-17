package com.opensaab.usb;
public final class SecurityPreviewPolicyTest {
 static void check(boolean value,String name){if(!value)throw new AssertionError(name);}
 public static void main(String[] args){
  check(SecurityPreviewPolicy.noPrompt("0.1.0-preview.95","0.1.0-preview.95","developer_preview","preview_no_prompt"),"published preview skips prompt");
  check(SecurityPreviewPolicy.noPrompt("0.1.0-headunit.18","0.1.0-headunit.18","developer_preview","preview_no_prompt"),"ARM32 preview shares policy");
  check(!SecurityPreviewPolicy.noPrompt("0.1.0","0.1.0","developer_preview","preview_no_prompt"),"qualified version remains protected");
  check(!SecurityPreviewPolicy.noPrompt("0.1.0-preview.95","0.1.0-preview.94","developer_preview","preview_no_prompt"),"metadata mismatch remains protected");
  check(!SecurityPreviewPolicy.noPrompt("0.1.0-preview.95","0.1.0-preview.95","qualified","preview_no_prompt"),"qualified channel remains protected");
  check(!SecurityPreviewPolicy.noPrompt("0.1.0-preview.95","0.1.0-preview.95","developer_preview",""),"missing policy remains protected");
  check(!SecurityPreviewPolicy.noPrompt(null,null,null,null),"unknown identity remains protected");
  System.out.println("SecurityPreviewPolicyTest: 7 checks passed");
 }
}
