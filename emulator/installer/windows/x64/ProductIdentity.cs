// SPDX-License-Identifier: MPL-2.0
using System;
using System.Collections.Generic;
using System.IO;
using System.Text.RegularExpressions;
using System.Web.Script.Serialization;

// The build embeds this record from the clean source commit. Renaming the EXE
// cannot change its displayed identity or the version reported at submission.
static class ProductIdentity
{
    static readonly Dictionary<string, object> metadata = Load();
    public static string Version { get { return (string)metadata["version"]; } }
    public static string Revision { get { return (string)metadata["source_revision"]; } }
    public static string Architecture { get { return (string)metadata["architecture"]; } }
    public static string ProgramName { get { return "OpenSAAB " + Version + " " + Architecture; } }
    public static bool PreviewSecurityNoPrompt { get { return SelectPreviewNoPrompt(metadata); } }
    internal static bool SelectPreviewNoPrompt(Dictionary<string, object> value)
    {
        object channel, mode;
        return value != null && value.TryGetValue("release_channel", out channel)
            && Object.Equals(channel, "developer_preview")
            && value.TryGetValue("security_access_mode", out mode)
            && Object.Equals(mode, "preview_no_prompt");
    }
    static Dictionary<string, object> Load()
    {
        using (var stream = typeof(ProductIdentity).Assembly.GetManifestResourceStream("opensaab-build.json"))
        {
            if (stream == null || stream.Length > 4096) throw new InvalidDataException("Packaged OpenSAAB identity is missing.");
            using (var reader = new StreamReader(stream))
            {
                var value = new JavaScriptSerializer().Deserialize<Dictionary<string, object>>(reader.ReadToEnd());
                if (!Object.Equals(value["product"], "OpenSAAB") || !Object.Equals(value["architecture"], "x86_64")
                    || !(value["version"] is string) || !Regex.IsMatch((string)value["version"], @"\A[0-9]+\.[0-9]+\.[0-9]+-preview\.[0-9]+\z")
                    || !(value["source_revision"] is string) || !Regex.IsMatch((string)value["source_revision"], @"\A[0-9a-f]{40}\z"))
                    throw new InvalidDataException("Packaged OpenSAAB identity is invalid.");
                return value;
            }
        }
    }
}
