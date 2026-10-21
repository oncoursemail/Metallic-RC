// Bounded behavioral reconstruction from build 23191908, NOT original source.
// Source methods 387023 / 387055; helper methods 380446 / 380444.
// Native operations were inspected with REA. Tests execute only this new code.
// This is not wired into the game or counted as a recovered whole-game build.
namespace Metallic.Reconstruction;

public static class ObservedByteCodec
{
    public static int ReadInt32(ReadOnlySpan<byte> source, int index)
    {
        // Keep individual access order visible. Do not infer a whole-buffer format.
        byte a = source[index];
        byte b = source[unchecked(index + 1)];
        byte c = source[unchecked(index + 2)];
        byte d = source[unchecked(index + 3)];
        return unchecked(a | (b << 8) | (c << 16) | (d << 24));
    }

    public static void WriteInt32(byte[] destination, int index, int value)
    {
        // The observed routine stores before checking the next position.
        // A single up-front bounds check would change partial-write behavior.
        destination[index] = unchecked((byte)value);
        destination[unchecked(index + 1)] = unchecked((byte)(value >> 8));
        destination[unchecked(index + 2)] = unchecked((byte)(value >> 16));
        destination[unchecked(index + 3)] = unchecked((byte)(value >> 24));
    }
}
