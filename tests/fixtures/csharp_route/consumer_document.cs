// FCB-009/FCB-022 consumer verification document: C# route.
// Exercises: verbatim/interpolated/raw strings, nested interpolation,
// character escapes, preprocessor directives, comments, and numbers.

#nullable enable
using System;
using System.Collections.Generic;

namespace Acme.SourceBrowser
{
    public class DocumentProcessor
    {
        /* Multi-line block comment:
           exercises block comment continuation across splits */
        private readonly string _name;
        private int _counter = 0;

        public DocumentProcessor(string name)
        {
            _name = name ?? throw new ArgumentNullException(nameof(name));
        }

        public void Process()
        {
            var regular = "Simple string with \"escaped\" quotes\n";
            var verbatim = @"Verbatim path C:\Data\logs with ""doubled"" quotes";
            var raw = """
                Multi-line raw string literal
                with "embedded" content and no escapes
                """;
            var interpolated = $"Processed: {regular.Length} items for {_name.ToUpper()}";
            var nested = $"Complex: {items.Find(x => x.Name == "sub_\"key\"")} done";

            char singleChar = 'a';
            char escapedNewline = '\n';
            char unicodeChar = '\u0041';

            ulong hexVal = 0xFE_ED_ul;
            int binVal = 0b1011_0100;
            decimal decVal = 123.45m;
            float expVal = 3.14e-2f;

            if (decVal > 0m)
            {
                _counter++;
            }
        }
    }
}
