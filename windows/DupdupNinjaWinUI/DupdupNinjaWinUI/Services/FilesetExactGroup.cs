using System.Collections.Generic;

namespace DupdupNinjaWinUI.Services;

public sealed class FilesetExactGroup
{
    public required string Label { get; init; }

    public required IReadOnlyList<FilesetResultRow> Rows { get; init; }
}
