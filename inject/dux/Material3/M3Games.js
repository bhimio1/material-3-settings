.pragma library
// Shared bits of the Material 3 Games integration.

var prelude = 'PATH="$HOME/.local/bin:$PATH"';

/** argv for running m3-games with arguments (never through string interpolation). */
function command(args) {
    return ["bash", "-c", prelude + '; exec m3-games "$@"', "m3-games"].concat(args || []);
}

function parseLibrary(text) {
    try {
        var parsed = JSON.parse(text);
        return Array.isArray(parsed) ? parsed : [];
    } catch (e) {
        return [];
    }
}
