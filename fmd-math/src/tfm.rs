//! TeX's font metrics for the Computer Modern and AMS faces fmd-math sets
//! mathematics in (generated; do not edit by hand).
//!
//! Written by `scripts/font-oracles/extract-cm-tfm.py` from TeX Live's
//! `fonts/tfm/public`: Knuth's Computer Modern metrics from `cm`, cmex7
//! from `amsfonts/cmextra`, AMS from `amsfonts/symbols` and Euler
//! Fraktur from `amsfonts/euler`.
//! AMS symbol and Euler metrics carry the original AMS OFL notice in
//! `OFL-AMS.txt`.
//! Values are the TFM fix_words over 2^20,
//! in ems of each font's design size, so they are exact. Ligatures are
//! omitted: the Reference preamble disables them.
//!
//! Sources (sha256):
//! - `cmmi10.tfm` e442c5487f84df70218ff37f775c87060856f5b6e04c011b6cadbbadfcf46645
//! - `cmmi7.tfm` ce55681703aae565f17286d800ea9780223132589bdbaeda1751b869aba26a38
//! - `cmmi5.tfm` 8839e40838381a0a3c976bdd5becf72afd60bde5a965c3d9b985c3479c6cf75e
//! - `cmmi6.tfm` ab6ecb4aaba9ca1b4259b1d1f64309a785bbaeb09183fd79a066b7764a448a28
//! - `cmmi8.tfm` 65af8c1e162a952cab8e93c834fcf43ce09b508e6c16e512ebb673cd850b0d41
//! - `cmmi9.tfm` 20672649a2f07eef8fe07ccb42eeb731023e48f2e0797ecef57c0887da46c032
//! - `cmmi12.tfm` 79563547084d85388e6909888f9fa7f5cad33bdda44ca6b25b362e1d301db539
//! - `cmsy10.tfm` 0ca13d421ac7133271aed7c935099ecf3d1d08ac9e15f81acb34a16564ab8a46
//! - `cmsy7.tfm` 4f59bbf683e947c807158641621982dc83f55fec042fea7963d8ce9f9da98256
//! - `cmsy5.tfm` 6b7dc18cba9ca8996d7b146f1ed78d9227577b26248083310fec227692198ca5
//! - `cmsy6.tfm` 137eb9f38f661c66309614066a7b5d63d8969d33aba0c9efee9f844a4ce65b50
//! - `cmsy8.tfm` 84c37f07ca360c41ad0a0fba7387d279d72457c69bdae03653208c683d8de6bc
//! - `cmsy9.tfm` b57c5c42a7991a004ee1b40b340d7bd12cc75ab0a6f7daf7a207827504f91712
//! - `cmex10.tfm` 0890bccea1dd4d27f001ac30e86c63af35bc803e0557c35aafb1903c8d208e92
//! - `cmex7.tfm` 373172fe340e4aede5129b89d65f576bfb1fe6932bd55c38f60bcaa84f3d1188
//! - `cmex8.tfm` 3f56ab22f7fc6a015813976c6c6cc2fd55736bbaec4958a4562e10719fa18062
//! - `cmex9.tfm` 70ca3442c95cd6cf3437ab7c2d0e4b9797531eacfe400dccc61d04bda288bae0
//! - `cmr10.tfm` 87f2d8981927644cbecaf3d639e96e348ea4e7be49d8804468bd8ba9ff3f5244
//! - `cmr7.tfm` 28f9f4d237bdae4babef4a59d90f17686a09d6f9df0b2c3c23b2ba15f20eee82
//! - `cmr5.tfm` 581be818f67bd11a1792d05e129abe3ea30c5cbdfce3bed8da7c5fb50f5ba95b
//! - `cmr6.tfm` 106afa9172f0c00e58b9f5cbc72b302a18aaeeaddf50caca145af9cc0b81afd2
//! - `cmr8.tfm` 4c5ae243ac0aa254e7ec7f0602cc2a4351e5011ff0cb6dc150a5f6de19585de6
//! - `cmr9.tfm` 2ffa73e8446c1d46c3509a0f65ff97d4d09de46f48e1ce578a1004498b67dba9
//! - `cmr12.tfm` 1ce8af37ed38e93940829d3540e494eaaddc5201758d036b9074467e70a738dc
//! - `cmr17.tfm` 86b6e8a52aae6ad1655e32099e12bd2904b158aba4c8afb74143266d2d9cd18f
//! - `msam10.tfm` 3b54bde5cb0e0bd071eea7bc702ed3a1a284f786779ef8e75b06eba4104bb9f2
//! - `msam7.tfm` 719d100c110fa1cfae9ad0b63e6c21753f4980f925fa516c1a04961e206f69d5
//! - `msam5.tfm` c800d1dfd533040219fcc06d52e0e00c2a20fb9b0039fe9f58ccec53fd003a9f
//! - `msbm10.tfm` d9f5f519ec718e9dbccb8527c1f5d3b4a008dfb946377f97f71611b385d3d010
//! - `msbm7.tfm` 361b5530a4b410c6274e9330c11e993843c62e394323aeaa6e0aee61e8042444
//! - `msbm5.tfm` 9e0909a297a3097e50960158d324006ddd5302db665400fa1fdf966df14e1a17
//! - `cmti10.tfm` 46a66e937f809c4fbe317947b583a14b408aca41ecb835eca3be46391190432a
//! - `eufm10.tfm` f0b7018ade501b4647c116a9ed1df10612cf8c173d0657ea17c5f1f6003fd1a1
//! - `eufm7.tfm` 23cdb6f111156554ea02f45f63c38a31e8e591d497b3810e7c71304f866dde74
//! - `eufm5.tfm` a7293dbf5c5c9d6dcbe75405f1a628788e0bc23b0a00e7b289d21eae163f7285

/// One character's TFM box, in ems of the font's design size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TfmChar {
    /// Width.
    pub width: f64,
    /// Height above the baseline (negative for characters wholly below it).
    pub height: f64,
    /// Depth below the baseline (negative for characters wholly above it).
    pub depth: f64,
    /// Italic correction.
    pub italic: f64,
}

/// One TFM file: its characters by code, its kerns and the two values TeX
/// reads beside them.
#[derive(Debug)]
pub struct TfmFont {
    /// The TFM file's name.
    pub name: &'static str,
    /// fontdimen 2, the interword space: zero for cmmi, cmsy and cmex,
    /// which keeps rule 17's italic correction inside a run of letters.
    pub space: f64,
    /// The skewchar LaTeX assigns the font, if any.
    pub skew_char: Option<u8>,
    /// The fontdimens: `params[n - 1]` is fontdimen n, in ems of the
    /// design size (fontdimen 1, the slant, is a plain ratio).
    pub params: &'static [f64],
    /// Codes 0-127.
    pub chars: &'static [Option<TfmChar>; 128],
    /// The lig/kern program's kerns, as `(left, right, kern)` sorted by pair.
    pub kerns: &'static [(u8, u8, f64)],
}

impl TfmFont {
    /// The character at `code`, if the font has it.
    #[must_use]
    pub fn char(&self, code: u8) -> Option<TfmChar> {
        self.chars.get(usize::from(code)).copied().flatten()
    }

    /// Fontdimen `n` (1-based, as TeX numbers them), in ems of the
    /// design size.
    ///
    /// # Panics
    /// If the font has fewer than `n` fontdimens: a typo in a parameter
    /// table, never input data.
    #[must_use]
    pub const fn param(&self, n: usize) -> f64 {
        self.params[n - 1]
    }

    /// The kern the lig/kern program places between `left` and `right`.
    #[must_use]
    pub fn kern(&self, left: u8, right: u8) -> Option<f64> {
        self.kerns
            .binary_search_by(|&(l, r, _)| (l, r).cmp(&(left, right)))
            .ok()
            .map(|i| self.kerns[i].2)
    }

    /// An accent's skew over the character at `code`: its kern with the
    /// skewchar (tex.web §741), zero without one.
    #[must_use]
    pub fn skew(&self, code: u8) -> f64 {
        self.skew_char
            .and_then(|skew| self.kern(code, skew))
            .unwrap_or(0.0)
    }
}

/// `cmmi10.tfm`.
#[rustfmt::skip]
pub static CMMI10: TfmFont = TfmFont {
    name: "cmmi10",
    space: 0.0,
    skew_char: Some(0x7F),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305553436279297, 1.0000028610229492],
    chars: &[
        Some(TfmChar { width: 0.6152782440185547, height: 0.6833324432373047, depth: 0.0, italic: 0.13889026641845703 }), // 0x00
        Some(TfmChar { width: 0.8333358764648438, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.7627763748168945, height: 0.6833324432373047, depth: 0.0, italic: 0.02777862548828125 }), // 0x02
        Some(TfmChar { width: 0.6944456100463867, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.7423629760742188, height: 0.6833324432373047, depth: 0.0, italic: 0.07569408416748047 }), // 0x04
        Some(TfmChar { width: 0.8312511444091797, height: 0.6833324432373047, depth: 0.0, italic: 0.08124828338623047 }), // 0x05
        Some(TfmChar { width: 0.7798633575439453, height: 0.6833324432373047, depth: 0.0, italic: 0.05763816833496094 }), // 0x06
        Some(TfmChar { width: 0.5833330154418945, height: 0.6833324432373047, depth: 0.0, italic: 0.13889026641845703 }), // 0x07
        Some(TfmChar { width: 0.6666669845581055, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.6122226715087891, height: 0.6833324432373047, depth: 0.0, italic: 0.1100006103515625 }), // 0x09
        Some(TfmChar { width: 0.7723979949951172, height: 0.6833324432373047, depth: 0.0, italic: 0.05017280578613281 }), // 0x0A
        Some(TfmChar { width: 0.6397018432617188, height: 0.4305553436279297, depth: 0.0, italic: 0.0037021636962890625 }), // 0x0B
        Some(TfmChar { width: 0.5656261444091797, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.05277824401855469 }), // 0x0C
        Some(TfmChar { width: 0.517730712890625, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.05555534362792969 }), // 0x0D
        Some(TfmChar { width: 0.4444446563720703, height: 0.6944446563720703, depth: 0.0, italic: 0.03784656524658203 }), // 0x0E
        Some(TfmChar { width: 0.40590381622314453, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.4375019073486328, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.07378387451171875 }), // 0x10
        Some(TfmChar { width: 0.49653148651123047, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x11
        Some(TfmChar { width: 0.46944427490234375, height: 0.6944446563720703, depth: 0.0, italic: 0.02777862548828125 }), // 0x12
        Some(TfmChar { width: 0.35393714904785156, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5761594772338867, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5833358764648438, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6025495529174805, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.49398326873779297, height: 0.4305553436279297, depth: 0.0, italic: 0.06365776062011719 }), // 0x17
        Some(TfmChar { width: 0.4375019073486328, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.04600715637207031 }), // 0x18
        Some(TfmChar { width: 0.5700273513793945, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x19
        Some(TfmChar { width: 0.5170154571533203, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.5714139938354492, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x1B
        Some(TfmChar { width: 0.43715476989746094, height: 0.4305553436279297, depth: 0.0, italic: 0.11319541931152344 }), // 0x1C
        Some(TfmChar { width: 0.5402803421020508, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x1D
        Some(TfmChar { width: 0.5958347320556641, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.6256923675537109, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.6513919830322266, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x20
        Some(TfmChar { width: 0.6224546432495117, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x21
        Some(TfmChar { width: 0.46631813049316406, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.5914402008056641, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.828129768371582, height: 0.4305553436279297, depth: 0.0, italic: 0.02777862548828125 }), // 0x24
        Some(TfmChar { width: 0.5170154571533203, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.36284828186035156, height: 0.4305553436279297, depth: 0.09722328186035156, italic: 0.07985973358154297 }), // 0x26
        Some(TfmChar { width: 0.6541671752929688, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.27777862548828125, height: 0.4637479782104492, depth: -0.03625202178955078, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.27777862548828125, height: 0.4637479782104492, depth: -0.03625202178955078, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.5000019073486328, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.5000019073486328, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.27777862548828125, height: 0.10555553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.27777862548828125, height: 0.10555553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.7777805328369141, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.7777805328369141, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.5000019073486328, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.5309038162231445, height: 0.6944446563720703, depth: 0.0, italic: 0.05555534362792969 }), // 0x40
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.758509635925293, height: 0.6833324432373047, depth: 0.0, italic: 0.05017280578613281 }), // 0x42
        Some(TfmChar { width: 0.7147216796875, height: 0.6833324432373047, depth: 0.0, italic: 0.07152748107910156 }), // 0x43
        Some(TfmChar { width: 0.8279170989990234, height: 0.6833324432373047, depth: 0.0, italic: 0.02777862548828125 }), // 0x44
        Some(TfmChar { width: 0.7381954193115234, height: 0.6833324432373047, depth: 0.0, italic: 0.05763816833496094 }), // 0x45
        Some(TfmChar { width: 0.6430568695068359, height: 0.6833324432373047, depth: 0.0, italic: 0.13889026641845703 }), // 0x46
        Some(TfmChar { width: 0.7862491607666016, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.8312511444091797, height: 0.6833324432373047, depth: 0.0, italic: 0.08124828338623047 }), // 0x48
        Some(TfmChar { width: 0.43958473205566406, height: 0.6833324432373047, depth: 0.0, italic: 0.07847118377685547 }), // 0x49
        Some(TfmChar { width: 0.5545139312744141, height: 0.6833324432373047, depth: 0.0, italic: 0.09618091583251953 }), // 0x4A
        Some(TfmChar { width: 0.8493070602416992, height: 0.6833324432373047, depth: 0.0, italic: 0.07152748107910156 }), // 0x4B
        Some(TfmChar { width: 0.6805572509765625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.9701404571533203, height: 0.6833324432373047, depth: 0.0, italic: 0.10902690887451172 }), // 0x4D
        Some(TfmChar { width: 0.8034725189208984, height: 0.6833324432373047, depth: 0.0, italic: 0.10902690887451172 }), // 0x4E
        Some(TfmChar { width: 0.7627763748168945, height: 0.6833324432373047, depth: 0.0, italic: 0.02777862548828125 }), // 0x4F
        Some(TfmChar { width: 0.6420135498046875, height: 0.6833324432373047, depth: 0.0, italic: 0.13889026641845703 }), // 0x50
        Some(TfmChar { width: 0.7905550003051758, height: 0.6833324432373047, depth: 0.1944446563720703, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7592897415161133, height: 0.6833324432373047, depth: 0.0, italic: 0.007725715637207031 }), // 0x52
        Some(TfmChar { width: 0.6131954193115234, height: 0.6833324432373047, depth: 0.0, italic: 0.05763816833496094 }), // 0x53
        Some(TfmChar { width: 0.584376335144043, height: 0.6833324432373047, depth: 0.0, italic: 0.13889026641845703 }), // 0x54
        Some(TfmChar { width: 0.6827774047851562, height: 0.6833324432373047, depth: 0.0, italic: 0.10902690887451172 }), // 0x55
        Some(TfmChar { width: 0.5833339691162109, height: 0.6833324432373047, depth: 0.0, italic: 0.22222328186035156 }), // 0x56
        Some(TfmChar { width: 0.9444456100463867, height: 0.6833324432373047, depth: 0.0, italic: 0.13889026641845703 }), // 0x57
        Some(TfmChar { width: 0.8284740447998047, height: 0.6833324432373047, depth: 0.0, italic: 0.07847118377685547 }), // 0x58
        Some(TfmChar { width: 0.5805568695068359, height: 0.6833324432373047, depth: 0.0, italic: 0.22222328186035156 }), // 0x59
        Some(TfmChar { width: 0.6826400756835938, height: 0.6833324432373047, depth: 0.0, italic: 0.07152748107910156 }), // 0x5A
        Some(TfmChar { width: 0.38889026641845703, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.38889026641845703, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.38889026641845703, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.0000028610229492, height: 0.35764026641845703, depth: -0.14235973358154297, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.0000028610229492, height: 0.35764026641845703, depth: -0.14235973358154297, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.4166698455810547, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.528590202331543, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.4291667938232422, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.4327564239501953, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.5204877853393555, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.46562671661376953, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.48958587646484375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.10764026641845703 }), // 0x66
        Some(TfmChar { width: 0.47696876525878906, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x67
        Some(TfmChar { width: 0.5761594772338867, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.344512939453125, height: 0.6595249176025391, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.4118070602416992, height: 0.6595249176025391, depth: 0.1944446563720703, italic: 0.05724334716796875 }), // 0x6A
        Some(TfmChar { width: 0.520604133605957, height: 0.6944446563720703, depth: 0.0, italic: 0.03148078918457031 }), // 0x6B
        Some(TfmChar { width: 0.29837989807128906, height: 0.6944446563720703, depth: 0.0, italic: 0.019678115844726562 }), // 0x6C
        Some(TfmChar { width: 0.8780136108398438, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.6002349853515625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.4847230911254883, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5031261444091797, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.4464139938354492, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x71
        Some(TfmChar { width: 0.4511594772338867, height: 0.4305553436279297, depth: 0.0, italic: 0.02777862548828125 }), // 0x72
        Some(TfmChar { width: 0.46875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.3611125946044922, height: 0.6150798797607422, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.5724582672119141, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.4847240447998047, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x76
        Some(TfmChar { width: 0.7159175872802734, height: 0.4305553436279297, depth: 0.0, italic: 0.02690887451171875 }), // 0x77
        Some(TfmChar { width: 0.571528434753418, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.4902820587158203, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x79
        Some(TfmChar { width: 0.46504974365234375, height: 0.4305553436279297, depth: 0.0, italic: 0.04398059844970703 }), // 0x7A
        Some(TfmChar { width: 0.32245635986328125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.3840303421020508, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.6364593505859375, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.5000019073486328, height: 0.7144441604614258, depth: 0.0, italic: 0.15381908416748047 }), // 0x7E
        Some(TfmChar { width: 0.27777862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.3994617462158203 }), // 0x7F
    ],
    kerns: &[
        (0x00, 0x3A, -0.11111164093017578),
        (0x00, 0x3B, -0.11111164093017578),
        (0x00, 0x3D, -0.05555534362792969),
        (0x00, 0x7F, 0.08333587646484375),
        (0x01, 0x7F, 0.1666717529296875),
        (0x02, 0x7F, 0.08333587646484375),
        (0x03, 0x7F, 0.1666717529296875),
        (0x04, 0x7F, 0.08333587646484375),
        (0x05, 0x3A, -0.05555534362792969),
        (0x05, 0x3B, -0.05555534362792969),
        (0x05, 0x3D, -0.05555534362792969),
        (0x05, 0x7F, 0.0555572509765625),
        (0x06, 0x7F, 0.08333587646484375),
        (0x07, 0x3A, -0.11111164093017578),
        (0x07, 0x3B, -0.11111164093017578),
        (0x07, 0x3D, -0.05555534362792969),
        (0x07, 0x7F, 0.0555572509765625),
        (0x08, 0x7F, 0.08333587646484375),
        (0x09, 0x3A, -0.05555534362792969),
        (0x09, 0x3B, -0.05555534362792969),
        (0x09, 0x3D, -0.05555534362792969),
        (0x09, 0x7F, 0.0555572509765625),
        (0x0A, 0x7F, 0.08333587646484375),
        (0x0B, 0x7F, 0.02777862548828125),
        (0x0C, 0x7F, 0.08333587646484375),
        (0x0E, 0x3A, -0.05555534362792969),
        (0x0E, 0x3B, -0.05555534362792969),
        (0x0E, 0x7F, 0.0555572509765625),
        (0x0F, 0x7F, 0.0555572509765625),
        (0x10, 0x7F, 0.08333587646484375),
        (0x11, 0x7F, 0.0555572509765625),
        (0x12, 0x7F, 0.08333587646484375),
        (0x13, 0x7F, 0.0555572509765625),
        (0x16, 0x7F, 0.02777862548828125),
        (0x17, 0x3A, -0.05555534362792969),
        (0x17, 0x3B, -0.05555534362792969),
        (0x17, 0x7F, 0.02777862548828125),
        (0x18, 0x7F, 0.111114501953125),
        (0x1A, 0x7F, 0.08333587646484375),
        (0x1B, 0x3A, -0.05555534362792969),
        (0x1B, 0x3B, -0.05555534362792969),
        (0x1C, 0x3A, -0.05555534362792969),
        (0x1C, 0x3B, -0.05555534362792969),
        (0x1C, 0x7F, 0.02777862548828125),
        (0x1D, 0x7F, 0.02777862548828125),
        (0x1E, 0x7F, 0.08333587646484375),
        (0x1F, 0x7F, 0.0555572509765625),
        (0x20, 0x7F, 0.111114501953125),
        (0x22, 0x7F, 0.08333587646484375),
        (0x23, 0x7F, 0.08333587646484375),
        (0x25, 0x7F, 0.08333587646484375),
        (0x26, 0x7F, 0.08333587646484375),
        (0x27, 0x7F, 0.08333587646484375),
        (0x3D, 0x01, -0.05555534362792969),
        (0x3D, 0x41, -0.05555534362792969),
        (0x3D, 0x4D, -0.05555534362792969),
        (0x3D, 0x4E, -0.05555534362792969),
        (0x3D, 0x59, 0.05555534362792969),
        (0x3D, 0x5A, -0.05555534362792969),
        (0x40, 0x7F, 0.08333587646484375),
        (0x41, 0x7F, 0.13889312744140625),
        (0x42, 0x7F, 0.08333587646484375),
        (0x43, 0x3A, -0.05555534362792969),
        (0x43, 0x3B, -0.05555534362792969),
        (0x43, 0x3D, -0.02777862548828125),
        (0x43, 0x7F, 0.08333587646484375),
        (0x44, 0x7F, 0.0555572509765625),
        (0x45, 0x7F, 0.08333587646484375),
        (0x46, 0x3A, -0.11111164093017578),
        (0x46, 0x3B, -0.11111164093017578),
        (0x46, 0x3D, -0.05555534362792969),
        (0x46, 0x7F, 0.08333587646484375),
        (0x47, 0x7F, 0.08333587646484375),
        (0x48, 0x3A, -0.05555534362792969),
        (0x48, 0x3B, -0.05555534362792969),
        (0x48, 0x3D, -0.05555534362792969),
        (0x48, 0x7F, 0.0555572509765625),
        (0x49, 0x7F, 0.111114501953125),
        (0x4A, 0x3A, -0.11111164093017578),
        (0x4A, 0x3B, -0.11111164093017578),
        (0x4A, 0x3D, -0.05555534362792969),
        (0x4A, 0x7F, 0.1666717529296875),
        (0x4B, 0x3A, -0.05555534362792969),
        (0x4B, 0x3B, -0.05555534362792969),
        (0x4B, 0x3D, -0.05555534362792969),
        (0x4B, 0x7F, 0.0555572509765625),
        (0x4C, 0x7F, 0.02777862548828125),
        (0x4D, 0x3A, -0.05555534362792969),
        (0x4D, 0x3B, -0.05555534362792969),
        (0x4D, 0x3D, -0.05555534362792969),
        (0x4D, 0x7F, 0.08333587646484375),
        (0x4E, 0x3A, -0.05555534362792969),
        (0x4E, 0x3B, -0.05555534362792969),
        (0x4E, 0x3D, -0.08333396911621094),
        (0x4E, 0x7F, 0.08333587646484375),
        (0x4F, 0x7F, 0.08333587646484375),
        (0x50, 0x3A, -0.11111164093017578),
        (0x50, 0x3B, -0.11111164093017578),
        (0x50, 0x3D, -0.05555534362792969),
        (0x50, 0x7F, 0.08333587646484375),
        (0x51, 0x7F, 0.08333587646484375),
        (0x52, 0x7F, 0.08333587646484375),
        (0x53, 0x3A, -0.05555534362792969),
        (0x53, 0x3B, -0.05555534362792969),
        (0x53, 0x3D, -0.05555534362792969),
        (0x53, 0x7F, 0.08333587646484375),
        (0x54, 0x3A, -0.05555534362792969),
        (0x54, 0x3B, -0.05555534362792969),
        (0x54, 0x3D, -0.02777862548828125),
        (0x54, 0x7F, 0.08333587646484375),
        (0x55, 0x3A, -0.11111164093017578),
        (0x55, 0x3B, -0.11111164093017578),
        (0x55, 0x3D, -0.05555534362792969),
        (0x55, 0x7F, 0.02777862548828125),
        (0x56, 0x3A, -0.16666698455810547),
        (0x56, 0x3B, -0.16666698455810547),
        (0x56, 0x3D, -0.11111164093017578),
        (0x57, 0x3A, -0.16666698455810547),
        (0x57, 0x3B, -0.16666698455810547),
        (0x57, 0x3D, -0.11111164093017578),
        (0x58, 0x3A, -0.05555534362792969),
        (0x58, 0x3B, -0.05555534362792969),
        (0x58, 0x3D, -0.08333396911621094),
        (0x58, 0x7F, 0.08333587646484375),
        (0x59, 0x3A, -0.16666698455810547),
        (0x59, 0x3B, -0.16666698455810547),
        (0x59, 0x3D, -0.11111164093017578),
        (0x5A, 0x3A, -0.05555534362792969),
        (0x5A, 0x3B, -0.05555534362792969),
        (0x5A, 0x3D, -0.05555534362792969),
        (0x5A, 0x7F, 0.08333587646484375),
        (0x60, 0x7F, 0.111114501953125),
        (0x63, 0x7F, 0.0555572509765625),
        (0x64, 0x59, 0.05555534362792969),
        (0x64, 0x5A, -0.05555534362792969),
        (0x64, 0x66, -0.16666698455810547),
        (0x64, 0x6A, -0.11111164093017578),
        (0x64, 0x7F, 0.1666717529296875),
        (0x65, 0x7F, 0.0555572509765625),
        (0x66, 0x3A, -0.05555534362792969),
        (0x66, 0x3B, -0.05555534362792969),
        (0x66, 0x7F, 0.1666717529296875),
        (0x67, 0x7F, 0.02777862548828125),
        (0x68, 0x7F, -0.02777862548828125),
        (0x6A, 0x3A, -0.05555534362792969),
        (0x6A, 0x3B, -0.05555534362792969),
        (0x6C, 0x7F, 0.08333587646484375),
        (0x6F, 0x7F, 0.0555572509765625),
        (0x70, 0x7F, 0.08333587646484375),
        (0x71, 0x7F, 0.08333587646484375),
        (0x72, 0x3A, -0.05555534362792969),
        (0x72, 0x3B, -0.05555534362792969),
        (0x72, 0x7F, 0.0555572509765625),
        (0x73, 0x7F, 0.0555572509765625),
        (0x74, 0x7F, 0.08333587646484375),
        (0x75, 0x7F, 0.02777862548828125),
        (0x76, 0x7F, 0.02777862548828125),
        (0x77, 0x7F, 0.08333587646484375),
        (0x78, 0x7F, 0.02777862548828125),
        (0x79, 0x7F, 0.0555572509765625),
        (0x7A, 0x7F, 0.0555572509765625),
        (0x7B, 0x7F, 0.02777862548828125),
        (0x7C, 0x7F, 0.08333587646484375),
        (0x7D, 0x7F, 0.111114501953125),
    ],
};

/// `cmmi7.tfm`.
#[rustfmt::skip]
pub static CMMI7: TfmFont = TfmFont {
    name: "cmmi7",
    space: 0.0,
    skew_char: Some(0x7F),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305553436279297, 1.1706409454345703],
    chars: &[
        Some(TfmChar { width: 0.6937541961669922, height: 0.6833343505859375, depth: 0.0, italic: 0.15377140045166016 }), // 0x00
        Some(TfmChar { width: 0.9543695449829102, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.8689289093017578, height: 0.6833343505859375, depth: 0.0, italic: 0.030755043029785156 }), // 0x02
        Some(TfmChar { width: 0.7976245880126953, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.8444976806640625, height: 0.6833343505859375, depth: 0.0, italic: 0.07465267181396484 }), // 0x04
        Some(TfmChar { width: 0.9356212615966797, height: 0.6833343505859375, depth: 0.0, italic: 0.07648563385009766 }), // 0x05
        Some(TfmChar { width: 0.8863134384155273, height: 0.6833343505859375, depth: 0.0, italic: 0.05496025085449219 }), // 0x06
        Some(TfmChar { width: 0.6775798797607422, height: 0.6833343505859375, depth: 0.0, italic: 0.15377140045166016 }), // 0x07
        Some(TfmChar { width: 0.7698431015014648, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.7168865203857422, height: 0.6833343505859375, depth: 0.0, italic: 0.11446475982666016 }), // 0x09
        Some(TfmChar { width: 0.880040168762207, height: 0.6833343505859375, depth: 0.0, italic: 0.048684120178222656 }), // 0x0A
        Some(TfmChar { width: 0.742680549621582, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.6477727890014648, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.05138969421386719 }), // 0x0C
        Some(TfmChar { width: 0.6000757217407227, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0615081787109375 }), // 0x0D
        Some(TfmChar { width: 0.5192480087280273, height: 0.6944446563720703, depth: 0.0, italic: 0.03625965118408203 }), // 0x0E
        Some(TfmChar { width: 0.47614383697509766, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.5198431015014648, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.06485462188720703 }), // 0x10
        Some(TfmChar { width: 0.588597297668457, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x11
        Some(TfmChar { width: 0.5441455841064453, height: 0.6944446563720703, depth: 0.0, italic: 0.030755043029785156 }), // 0x12
        Some(TfmChar { width: 0.4227886199951172, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.6688213348388672, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.6775836944580078, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6946172714233398, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.5727558135986328, height: 0.4305553436279297, depth: 0.0, italic: 0.0666351318359375 }), // 0x17
        Some(TfmChar { width: 0.5198431015014648, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.03410148620605469 }), // 0x18
        Some(TfmChar { width: 0.6680450439453125, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x19
        Some(TfmChar { width: 0.59271240234375, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.661992073059082, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x1B
        Some(TfmChar { width: 0.5268383026123047, height: 0.4305553436279297, depth: 0.0, italic: 0.11557865142822266 }), // 0x1C
        Some(TfmChar { width: 0.6329412460327148, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x1D
        Some(TfmChar { width: 0.6869068145751953, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.7137928009033203, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.7559576034545898, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x20
        Some(TfmChar { width: 0.7196807861328125, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x21
        Some(TfmChar { width: 0.53973388671875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.689854621887207, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.9499578475952148, height: 0.4305553436279297, depth: 0.0, italic: 0.030755043029785156 }), // 0x24
        Some(TfmChar { width: 0.59271240234375, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.4392375946044922, height: 0.4305553436279297, depth: 0.09722232818603516, italic: 0.07688426971435547 }), // 0x26
        Some(TfmChar { width: 0.7513933181762695, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.33928775787353516, height: 0.49387073516845703, depth: -0.006129264831542969, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.33928775787353516, height: 0.49387073516845703, depth: -0.006129264831542969, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.5853204727172852, height: 0.46527767181396484, depth: -0.034722328186035156, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.5853204727172852, height: 0.46527767181396484, depth: -0.034722328186035156, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.5853204727172852, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5853204727172852, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.33928775787353516, height: 0.11507987976074219, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.33928775787353516, height: 0.11507987976074219, depth: 0.1944446563720703, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.8928613662719727, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.5853204727172852, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.8928613662719727, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.5853204727172852, height: 0.46527767181396484, depth: -0.034722328186035156, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.6100702285766602, height: 0.6944446563720703, depth: 0.0, italic: 0.0615081787109375 }), // 0x40
        Some(TfmChar { width: 0.8591327667236328, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.8631763458251953, height: 0.6833343505859375, depth: 0.0, italic: 0.048684120178222656 }), // 0x42
        Some(TfmChar { width: 0.8193883895874023, height: 0.6833343505859375, depth: 0.0, italic: 0.070037841796875 }), // 0x43
        Some(TfmChar { width: 0.9340715408325195, height: 0.6833343505859375, depth: 0.0, italic: 0.030755043029785156 }), // 0x44
        Some(TfmChar { width: 0.8386945724487305, height: 0.6833343505859375, depth: 0.0, italic: 0.05496025085449219 }), // 0x45
        Some(TfmChar { width: 0.7245092391967773, height: 0.6833343505859375, depth: 0.0, italic: 0.15377140045166016 }), // 0x46
        Some(TfmChar { width: 0.8894262313842773, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.9356212615966797, height: 0.6833343505859375, depth: 0.0, italic: 0.07648563385009766 }), // 0x48
        Some(TfmChar { width: 0.5063037872314453, height: 0.6833343505859375, depth: 0.0, italic: 0.077728271484375 }), // 0x49
        Some(TfmChar { width: 0.632044792175293, height: 0.6833343505859375, depth: 0.0, italic: 0.0943450927734375 }), // 0x4A
        Some(TfmChar { width: 0.9599285125732422, height: 0.6833343505859375, depth: 0.0, italic: 0.070037841796875 }), // 0x4B
        Some(TfmChar { width: 0.7837352752685547, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.089390754699707, height: 0.6833343505859375, depth: 0.0, italic: 0.10724067687988281 }), // 0x4D
        Some(TfmChar { width: 0.9048662185668945, height: 0.6833343505859375, depth: 0.0, italic: 0.10724067687988281 }), // 0x4E
        Some(TfmChar { width: 0.8689289093017578, height: 0.6833343505859375, depth: 0.0, italic: 0.030755043029785156 }), // 0x4F
        Some(TfmChar { width: 0.7273340225219727, height: 0.6833343505859375, depth: 0.0, italic: 0.15377140045166016 }), // 0x50
        Some(TfmChar { width: 0.899683952331543, height: 0.6833343505859375, depth: 0.1944446563720703, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.8606090545654297, height: 0.6833343505859375, depth: 0.0, italic: 0.005120277404785156 }), // 0x52
        Some(TfmChar { width: 0.7014923095703125, height: 0.6833343505859375, depth: 0.0, italic: 0.05466175079345703 }), // 0x53
        Some(TfmChar { width: 0.6747541427612305, height: 0.6833343505859375, depth: 0.0, italic: 0.15377140045166016 }), // 0x54
        Some(TfmChar { width: 0.7782182693481445, height: 0.6833343505859375, depth: 0.0, italic: 0.10724067687988281 }), // 0x55
        Some(TfmChar { width: 0.6746082305908203, height: 0.6833343505859375, depth: 0.0, italic: 0.24603271484375 }), // 0x56
        Some(TfmChar { width: 1.0744104385375977, height: 0.6833343505859375, depth: 0.0, italic: 0.15377140045166016 }), // 0x57
        Some(TfmChar { width: 0.9368610382080078, height: 0.6833343505859375, depth: 0.0, italic: 0.077728271484375 }), // 0x58
        Some(TfmChar { width: 0.671534538269043, height: 0.6833343505859375, depth: 0.0, italic: 0.24603271484375 }), // 0x59
        Some(TfmChar { width: 0.778376579284668, height: 0.6833343505859375, depth: 0.0, italic: 0.070037841796875 }), // 0x5A
        Some(TfmChar { width: 0.46230411529541016, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.46230411529541016, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.46230411529541016, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.1388940811157227, height: 0.35764026641845703, depth: -0.14235973358154297, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.1388940811157227, height: 0.35764026641845703, depth: -0.14235973358154297, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.478179931640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.619664192199707, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.5023822784423828, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.5105352401733398, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.594696044921875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.542017936706543, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.5570507049560547, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.11210441589355469 }), // 0x66
        Some(TfmChar { width: 0.5573291778564453, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x67
        Some(TfmChar { width: 0.6688213348388672, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.40418529510498047, height: 0.6617059707641602, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.47272396087646484, height: 0.6617059707641602, depth: 0.1944446563720703, italic: 0.05778694152832031 }), // 0x6A
        Some(TfmChar { width: 0.6073131561279297, height: 0.6944446563720703, depth: 0.0, italic: 0.024139404296875 }), // 0x6B
        Some(TfmChar { width: 0.36127758026123047, height: 0.6944446563720703, depth: 0.0, italic: 0.005789756774902344 }), // 0x6C
        Some(TfmChar { width: 1.0137310028076172, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.7061901092529297, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.5638904571533203, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5889081954956055, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.5235986709594727, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x71
        Some(TfmChar { width: 0.5304279327392578, height: 0.4305553436279297, depth: 0.0, italic: 0.030755043029785156 }), // 0x72
        Some(TfmChar { width: 0.539189338684082, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.4315509796142578, height: 0.6150798797607422, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.6754369735717773, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.5714330673217773, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x76
        Some(TfmChar { width: 0.8264360427856445, height: 0.4305553436279297, depth: 0.0, italic: 0.026909828186035156 }), // 0x77
        Some(TfmChar { width: 0.6478204727172852, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.579371452331543, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x79
        Some(TfmChar { width: 0.545806884765625, height: 0.4305553436279297, depth: 0.0, italic: 0.04100513458251953 }), // 0x7A
        Some(TfmChar { width: 0.3986492156982422, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.4419708251953125, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.7301130294799805, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.5853204727172852, height: 0.7187299728393555, depth: 0.0, italic: 0.15381908416748047 }), // 0x7E
        Some(TfmChar { width: 0.33928775787353516, height: 0.6944446563720703, depth: 0.0, italic: 0.46236133575439453 }), // 0x7F
    ],
    kerns: &[
        (0x00, 0x3A, -0.123016357421875),
        (0x00, 0x3B, -0.123016357421875),
        (0x00, 0x3D, -0.0615081787109375),
        (0x00, 0x7F, 0.09226512908935547),
        (0x01, 0x7F, 0.18453121185302734),
        (0x02, 0x7F, 0.09226512908935547),
        (0x03, 0x7F, 0.18453121185302734),
        (0x04, 0x7F, 0.09226512908935547),
        (0x05, 0x3A, -0.0615081787109375),
        (0x05, 0x3B, -0.0615081787109375),
        (0x05, 0x3D, -0.0615081787109375),
        (0x05, 0x7F, 0.06151008605957031),
        (0x06, 0x7F, 0.09226512908935547),
        (0x07, 0x3A, -0.123016357421875),
        (0x07, 0x3B, -0.123016357421875),
        (0x07, 0x3D, -0.0615081787109375),
        (0x07, 0x7F, 0.06151008605957031),
        (0x08, 0x7F, 0.09226512908935547),
        (0x09, 0x3A, -0.0615081787109375),
        (0x09, 0x3B, -0.0615081787109375),
        (0x09, 0x3D, -0.0615081787109375),
        (0x09, 0x7F, 0.06151008605957031),
        (0x0A, 0x7F, 0.09226512908935547),
        (0x0B, 0x7F, 0.030755043029785156),
        (0x0C, 0x7F, 0.09226512908935547),
        (0x0E, 0x3A, -0.0615081787109375),
        (0x0E, 0x3B, -0.0615081787109375),
        (0x0E, 0x7F, 0.06151008605957031),
        (0x0F, 0x7F, 0.06151008605957031),
        (0x10, 0x7F, 0.09226512908935547),
        (0x11, 0x7F, 0.06151008605957031),
        (0x12, 0x7F, 0.09226512908935547),
        (0x13, 0x7F, 0.06151008605957031),
        (0x16, 0x7F, 0.030755043029785156),
        (0x17, 0x3A, -0.0615081787109375),
        (0x17, 0x3B, -0.0615081787109375),
        (0x17, 0x7F, 0.030755043029785156),
        (0x18, 0x7F, 0.12302112579345703),
        (0x1A, 0x7F, 0.09226512908935547),
        (0x1B, 0x3A, -0.0615081787109375),
        (0x1B, 0x3B, -0.0615081787109375),
        (0x1C, 0x3A, -0.0615081787109375),
        (0x1C, 0x3B, -0.0615081787109375),
        (0x1C, 0x7F, 0.030755043029785156),
        (0x1D, 0x7F, 0.030755043029785156),
        (0x1E, 0x7F, 0.09226512908935547),
        (0x1F, 0x7F, 0.06151008605957031),
        (0x20, 0x7F, 0.12302112579345703),
        (0x22, 0x7F, 0.09226512908935547),
        (0x23, 0x7F, 0.09226512908935547),
        (0x25, 0x7F, 0.09226512908935547),
        (0x26, 0x7F, 0.09226512908935547),
        (0x27, 0x7F, 0.09226512908935547),
        (0x3D, 0x01, -0.0615081787109375),
        (0x3D, 0x41, -0.0615081787109375),
        (0x3D, 0x4D, -0.0615081787109375),
        (0x3D, 0x4E, -0.0615081787109375),
        (0x3D, 0x59, 0.0615081787109375),
        (0x3D, 0x5A, -0.0615081787109375),
        (0x40, 0x7F, 0.09226512908935547),
        (0x41, 0x7F, 0.1537761688232422),
        (0x42, 0x7F, 0.09226512908935547),
        (0x43, 0x3A, -0.0615081787109375),
        (0x43, 0x3B, -0.0615081787109375),
        (0x43, 0x3D, -0.030755043029785156),
        (0x43, 0x7F, 0.09226512908935547),
        (0x44, 0x7F, 0.06151008605957031),
        (0x45, 0x7F, 0.09226512908935547),
        (0x46, 0x3A, -0.123016357421875),
        (0x46, 0x3B, -0.123016357421875),
        (0x46, 0x3D, -0.0615081787109375),
        (0x46, 0x7F, 0.09226512908935547),
        (0x47, 0x7F, 0.09226512908935547),
        (0x48, 0x3A, -0.0615081787109375),
        (0x48, 0x3B, -0.0615081787109375),
        (0x48, 0x3D, -0.0615081787109375),
        (0x48, 0x7F, 0.06151008605957031),
        (0x49, 0x7F, 0.12302112579345703),
        (0x4A, 0x3A, -0.123016357421875),
        (0x4A, 0x3B, -0.123016357421875),
        (0x4A, 0x3D, -0.0615081787109375),
        (0x4A, 0x7F, 0.18453121185302734),
        (0x4B, 0x3A, -0.0615081787109375),
        (0x4B, 0x3B, -0.0615081787109375),
        (0x4B, 0x3D, -0.0615081787109375),
        (0x4B, 0x7F, 0.06151008605957031),
        (0x4C, 0x7F, 0.030755043029785156),
        (0x4D, 0x3A, -0.0615081787109375),
        (0x4D, 0x3B, -0.0615081787109375),
        (0x4D, 0x3D, -0.0615081787109375),
        (0x4D, 0x7F, 0.09226512908935547),
        (0x4E, 0x3A, -0.0615081787109375),
        (0x4E, 0x3B, -0.0615081787109375),
        (0x4E, 0x3D, -0.09226322174072266),
        (0x4E, 0x7F, 0.09226512908935547),
        (0x4F, 0x7F, 0.09226512908935547),
        (0x50, 0x3A, -0.123016357421875),
        (0x50, 0x3B, -0.123016357421875),
        (0x50, 0x3D, -0.0615081787109375),
        (0x50, 0x7F, 0.09226512908935547),
        (0x51, 0x7F, 0.09226512908935547),
        (0x52, 0x7F, 0.09226512908935547),
        (0x53, 0x3A, -0.0615081787109375),
        (0x53, 0x3B, -0.0615081787109375),
        (0x53, 0x3D, -0.0615081787109375),
        (0x53, 0x7F, 0.09226512908935547),
        (0x54, 0x3A, -0.0615081787109375),
        (0x54, 0x3B, -0.0615081787109375),
        (0x54, 0x3D, -0.030755043029785156),
        (0x54, 0x7F, 0.09226512908935547),
        (0x55, 0x3A, -0.123016357421875),
        (0x55, 0x3B, -0.123016357421875),
        (0x55, 0x3D, -0.0615081787109375),
        (0x55, 0x7F, 0.030755043029785156),
        (0x56, 0x3A, -0.1845245361328125),
        (0x56, 0x3B, -0.1845245361328125),
        (0x56, 0x3D, -0.123016357421875),
        (0x57, 0x3A, -0.1845245361328125),
        (0x57, 0x3B, -0.1845245361328125),
        (0x57, 0x3D, -0.123016357421875),
        (0x58, 0x3A, -0.0615081787109375),
        (0x58, 0x3B, -0.0615081787109375),
        (0x58, 0x3D, -0.09226322174072266),
        (0x58, 0x7F, 0.09226512908935547),
        (0x59, 0x3A, -0.1845245361328125),
        (0x59, 0x3B, -0.1845245361328125),
        (0x59, 0x3D, -0.123016357421875),
        (0x5A, 0x3A, -0.0615081787109375),
        (0x5A, 0x3B, -0.0615081787109375),
        (0x5A, 0x3D, -0.0615081787109375),
        (0x5A, 0x7F, 0.09226512908935547),
        (0x60, 0x7F, 0.12302112579345703),
        (0x63, 0x7F, 0.06151008605957031),
        (0x64, 0x59, 0.0615081787109375),
        (0x64, 0x5A, -0.0615081787109375),
        (0x64, 0x66, -0.1845245361328125),
        (0x64, 0x6A, -0.123016357421875),
        (0x64, 0x7F, 0.18453121185302734),
        (0x65, 0x7F, 0.06151008605957031),
        (0x66, 0x3A, -0.0615081787109375),
        (0x66, 0x3B, -0.0615081787109375),
        (0x66, 0x7F, 0.18453121185302734),
        (0x67, 0x7F, 0.030755043029785156),
        (0x68, 0x7F, -0.030755043029785156),
        (0x6A, 0x3A, -0.0615081787109375),
        (0x6A, 0x3B, -0.0615081787109375),
        (0x6C, 0x7F, 0.09226512908935547),
        (0x6F, 0x7F, 0.06151008605957031),
        (0x70, 0x7F, 0.09226512908935547),
        (0x71, 0x7F, 0.09226512908935547),
        (0x72, 0x3A, -0.0615081787109375),
        (0x72, 0x3B, -0.0615081787109375),
        (0x72, 0x7F, 0.06151008605957031),
        (0x73, 0x7F, 0.06151008605957031),
        (0x74, 0x7F, 0.09226512908935547),
        (0x75, 0x7F, 0.030755043029785156),
        (0x76, 0x7F, 0.030755043029785156),
        (0x77, 0x7F, 0.09226512908935547),
        (0x78, 0x7F, 0.030755043029785156),
        (0x79, 0x7F, 0.06151008605957031),
        (0x7A, 0x7F, 0.06151008605957031),
        (0x7B, 0x7F, 0.030755043029785156),
        (0x7C, 0x7F, 0.09226512908935547),
        (0x7D, 0x7F, 0.12302112579345703),
    ],
};

/// `cmmi5.tfm`.
#[rustfmt::skip]
pub static CMMI5: TfmFont = TfmFont {
    name: "cmmi5",
    space: 0.0,
    skew_char: Some(0x7F),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305543899536133, 1.4722414016723633],
    chars: &[
        Some(TfmChar { width: 0.8326473236083984, height: 0.6833343505859375, depth: 0.0, italic: 0.173614501953125 }), // 0x00
        Some(TfmChar { width: 1.1527957916259766, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 1.0475130081176758, height: 0.6833343505859375, depth: 0.0, italic: 0.03472328186035156 }), // 0x02
        Some(TfmChar { width: 0.9666805267333984, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 1.017721176147461, height: 0.6833343505859375, depth: 0.0, italic: 0.07326316833496094 }), // 0x04
        Some(TfmChar { width: 1.1104307174682617, height: 0.6833343505859375, depth: 0.0, italic: 0.07430458068847656 }), // 0x05
        Some(TfmChar { width: 1.0652923583984375, height: 0.6833343505859375, depth: 0.0, italic: 0.05138874053955078 }), // 0x06
        Some(TfmChar { width: 0.8402891159057617, height: 0.6833343505859375, depth: 0.0, italic: 0.173614501953125 }), // 0x07
        Some(TfmChar { width: 0.9444580078125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.8934841156005859, height: 0.6833343505859375, depth: 0.0, italic: 0.12041950225830078 }), // 0x09
        Some(TfmChar { width: 1.0606050491333008, height: 0.6833343505859375, depth: 0.0, italic: 0.04670143127441406 }), // 0x0A
        Some(TfmChar { width: 0.9133205413818359, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.7906341552734375, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.05138874053955078 }), // 0x0C
        Some(TfmChar { width: 0.7469053268432617, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.06944561004638672 }), // 0x0D
        Some(TfmChar { width: 0.6541719436645508, height: 0.6944456100463867, depth: 0.0, italic: 0.03229045867919922 }), // 0x0E
        Some(TfmChar { width: 0.6135463714599609, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.6666746139526367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.05295085906982422 }), // 0x10
        Some(TfmChar { width: 0.7437620162963867, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.03587913513183594 }), // 0x11
        Some(TfmChar { width: 0.6770877838134766, height: 0.6944456100463867, depth: 0.0, italic: 0.03472328186035156 }), // 0x12
        Some(TfmChar { width: 0.549774169921875, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.8275575637817383, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.8402891159057617, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.8497800827026367, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.7120456695556641, height: 0.4305543899536133, depth: 0.0, italic: 0.0706024169921875 }), // 0x17
        Some(TfmChar { width: 0.6666746139526367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.018228530883789062 }), // 0x18
        Some(TfmChar { width: 0.831146240234375, height: 0.4305543899536133, depth: 0.0, italic: 0.03587913513183594 }), // 0x19
        Some(TfmChar { width: 0.7260494232177734, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.8151702880859375, height: 0.4305543899536133, depth: 0.0, italic: 0.03587913513183594 }), // 0x1B
        Some(TfmChar { width: 0.6816072463989258, height: 0.4305543899536133, depth: 0.0, italic: 0.11597251892089844 }), // 0x1C
        Some(TfmChar { width: 0.7916774749755859, height: 0.4305543899536133, depth: 0.0, italic: 0.03587913513183594 }), // 0x1D
        Some(TfmChar { width: 0.8416748046875, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.864593505859375, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.9305696487426758, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.03587913513183594 }), // 0x20
        Some(TfmChar { width: 0.8863554000854492, height: 0.4305543899536133, depth: 0.0, italic: 0.03587913513183594 }), // 0x21
        Some(TfmChar { width: 0.6746578216552734, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.8553371429443359, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.1448087692260742, height: 0.4305543899536133, depth: 0.0, italic: 0.03472328186035156 }), // 0x24
        Some(TfmChar { width: 0.7260494232177734, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.5781307220458984, height: 0.4305543899536133, depth: 0.09722328186035156, italic: 0.07291603088378906 }), // 0x26
        Some(TfmChar { width: 0.9180660247802734, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.45833778381347656, height: 0.5148067474365234, depth: 0.014806747436523438, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.45833778381347656, height: 0.5148067474365234, depth: 0.014806747436523438, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.7361202239990234, height: 0.46527671813964844, depth: -0.03472328186035156, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.7361202239990234, height: 0.46527671813964844, depth: -0.03472328186035156, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.7361202239990234, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.7361202239990234, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.7361202239990234, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.7361202239990234, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.7361202239990234, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.7361202239990234, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.7361202239990234, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.7361202239990234, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.7361202239990234, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.7361202239990234, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.45833778381347656, height: 0.122222900390625, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.45833778381347656, height: 0.122222900390625, depth: 0.19444561004638672, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.7361202239990234, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.7361202239990234, height: 0.46527671813964844, depth: -0.03472328186035156, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.74896240234375, height: 0.6944456100463867, depth: 0.0, italic: 0.06944561004638672 }), // 0x40
        Some(TfmChar { width: 1.0361270904541016, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 1.0369930267333984, height: 0.6833343505859375, depth: 0.0, italic: 0.04670143127441406 }), // 0x42
        Some(TfmChar { width: 0.9959869384765625, height: 0.6833343505859375, depth: 0.0, italic: 0.06805419921875 }), // 0x43
        Some(TfmChar { width: 1.1098756790161133, height: 0.6833343505859375, depth: 0.0, italic: 0.03472328186035156 }), // 0x44
        Some(TfmChar { width: 1.0069580078125, height: 0.6833343505859375, depth: 0.0, italic: 0.05138874053955078 }), // 0x45
        Some(TfmChar { width: 0.86737060546875, height: 0.6833343505859375, depth: 0.0, italic: 0.173614501953125 }), // 0x46
        Some(TfmChar { width: 1.0640411376953125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 1.1104307174682617, height: 0.6833343505859375, depth: 0.0, italic: 0.07430458068847656 }), // 0x48
        Some(TfmChar { width: 0.6267423629760742, height: 0.6833343505859375, depth: 0.0, italic: 0.0767364501953125 }), // 0x49
        Some(TfmChar { width: 0.7729244232177734, height: 0.6833343505859375, depth: 0.0, italic: 0.0951385498046875 }), // 0x4A
        Some(TfmChar { width: 1.138906478881836, height: 0.6833343505859375, depth: 0.0, italic: 0.06805419921875 }), // 0x4B
        Some(TfmChar { width: 0.9555692672729492, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.2840452194213867, height: 0.6833343505859375, depth: 0.0, italic: 0.10902690887451172 }), // 0x4D
        Some(TfmChar { width: 1.0757083892822266, height: 0.6833343505859375, depth: 0.0, italic: 0.10902690887451172 }), // 0x4E
        Some(TfmChar { width: 1.0475130081176758, height: 0.6833343505859375, depth: 0.0, italic: 0.03472328186035156 }), // 0x4F
        Some(TfmChar { width: 0.8753566741943359, height: 0.6833343505859375, depth: 0.0, italic: 0.173614501953125 }), // 0x50
        Some(TfmChar { width: 1.082235336303711, height: 0.6833343505859375, depth: 0.19444561004638672, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 1.0299625396728516, height: 0.6833343505859375, depth: 0.0, italic: 0.00164794921875 }), // 0x52
        Some(TfmChar { width: 0.85626220703125, height: 0.6833343505859375, depth: 0.0, italic: 0.05069255828857422 }), // 0x53
        Some(TfmChar { width: 0.8323030471801758, height: 0.6833343505859375, depth: 0.0, italic: 0.173614501953125 }), // 0x54
        Some(TfmChar { width: 0.9438991546630859, height: 0.6833343505859375, depth: 0.0, italic: 0.10902690887451172 }), // 0x55
        Some(TfmChar { width: 0.827789306640625, height: 0.6833343505859375, depth: 0.0, italic: 0.2777833938598633 }), // 0x56
        Some(TfmChar { width: 1.2791872024536133, height: 0.6833343505859375, depth: 0.0, italic: 0.173614501953125 }), // 0x57
        Some(TfmChar { width: 1.112863540649414, height: 0.6833343505859375, depth: 0.0, italic: 0.0767364501953125 }), // 0x58
        Some(TfmChar { width: 0.8243160247802734, height: 0.6833343505859375, depth: 0.0, italic: 0.2777833938598633 }), // 0x59
        Some(TfmChar { width: 0.9430694580078125, height: 0.6833343505859375, depth: 0.0, italic: 0.06805419921875 }), // 0x5A
        Some(TfmChar { width: 0.59722900390625, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.59722900390625, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.59722900390625, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.3611326217651367, height: 0.3576383590698242, depth: -0.14236164093017578, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.3611326217651367, height: 0.3576383590698242, depth: -0.14236164093017578, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.5972261428833008, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.7744293212890625, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.6333370208740234, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.6494264602661133, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.7397060394287109, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.6770172119140625, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.6840333938598633, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.1180572509765625 }), // 0x66
        Some(TfmChar { width: 0.7005863189697266, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.03587913513183594 }), // 0x67
        Some(TfmChar { width: 0.8275575637817383, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.5335721969604492, height: 0.6664676666259766, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.5881986618041992, height: 0.6664676666259766, depth: 0.19444561004638672, italic: 0.05897808074951172 }), // 0x6A
        Some(TfmChar { width: 0.7581119537353516, height: 0.6944456100463867, depth: 0.0, italic: 0.01620197296142578 }), // 0x6B
        Some(TfmChar { width: 0.4803285598754883, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 1.2280302047729492, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.8808012008666992, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.7027835845947266, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.7397098541259766, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.6589174270629883, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.03587913513183594 }), // 0x71
        Some(TfmChar { width: 0.6713047027587891, height: 0.4305543899536133, depth: 0.0, italic: 0.03472328186035156 }), // 0x72
        Some(TfmChar { width: 0.6701450347900391, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.5636653900146484, height: 0.6150789260864258, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.8460788726806641, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.7222318649291992, height: 0.4305543899536133, depth: 0.0, italic: 0.03587913513183594 }), // 0x76
        Some(TfmChar { width: 1.0089845657348633, height: 0.4305543899536133, depth: 0.0, italic: 0.026910781860351562 }), // 0x77
        Some(TfmChar { width: 0.7916746139526367, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.7305660247802734, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.03587913513183594 }), // 0x79
        Some(TfmChar { width: 0.6886653900146484, height: 0.4305543899536133, depth: 0.0, italic: 0.03703594207763672 }), // 0x7A
        Some(TfmChar { width: 0.5335721969604492, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.5534763336181641, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.8892488479614258, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.7361202239990234, height: 0.7224454879760742, depth: 0.0, italic: 0.15382099151611328 }), // 0x7E
        Some(TfmChar { width: 0.45833778381347656, height: 0.6944456100463867, depth: 0.0, italic: 0.5814085006713867 }), // 0x7F
    ],
    kerns: &[
        (0x00, 0x3A, -0.13889122009277344),
        (0x00, 0x3B, -0.13889122009277344),
        (0x00, 0x3D, -0.06944561004638672),
        (0x00, 0x7F, 0.10416889190673828),
        (0x01, 0x7F, 0.20833778381347656),
        (0x02, 0x7F, 0.10416889190673828),
        (0x03, 0x7F, 0.20833778381347656),
        (0x04, 0x7F, 0.10416889190673828),
        (0x05, 0x3A, -0.06944561004638672),
        (0x05, 0x3B, -0.06944561004638672),
        (0x05, 0x3D, -0.06944561004638672),
        (0x05, 0x7F, 0.06944561004638672),
        (0x06, 0x7F, 0.10416889190673828),
        (0x07, 0x3A, -0.13889122009277344),
        (0x07, 0x3B, -0.13889122009277344),
        (0x07, 0x3D, -0.06944561004638672),
        (0x07, 0x7F, 0.06944561004638672),
        (0x08, 0x7F, 0.10416889190673828),
        (0x09, 0x3A, -0.06944561004638672),
        (0x09, 0x3B, -0.06944561004638672),
        (0x09, 0x3D, -0.06944561004638672),
        (0x09, 0x7F, 0.06944561004638672),
        (0x0A, 0x7F, 0.10416889190673828),
        (0x0B, 0x7F, 0.03472328186035156),
        (0x0C, 0x7F, 0.10416889190673828),
        (0x0E, 0x3A, -0.06944561004638672),
        (0x0E, 0x3B, -0.06944561004638672),
        (0x0E, 0x7F, 0.06944561004638672),
        (0x0F, 0x7F, 0.06944561004638672),
        (0x10, 0x7F, 0.10416889190673828),
        (0x11, 0x7F, 0.06944561004638672),
        (0x12, 0x7F, 0.10416889190673828),
        (0x13, 0x7F, 0.06944561004638672),
        (0x16, 0x7F, 0.03472328186035156),
        (0x17, 0x3A, -0.06944561004638672),
        (0x17, 0x3B, -0.06944561004638672),
        (0x17, 0x7F, 0.03472328186035156),
        (0x18, 0x7F, 0.13889122009277344),
        (0x1A, 0x7F, 0.10416889190673828),
        (0x1B, 0x3A, -0.06944561004638672),
        (0x1B, 0x3B, -0.06944561004638672),
        (0x1C, 0x3A, -0.06944561004638672),
        (0x1C, 0x3B, -0.06944561004638672),
        (0x1C, 0x7F, 0.03472328186035156),
        (0x1D, 0x7F, 0.03472328186035156),
        (0x1E, 0x7F, 0.10416889190673828),
        (0x1F, 0x7F, 0.06944561004638672),
        (0x20, 0x7F, 0.13889122009277344),
        (0x22, 0x7F, 0.10416889190673828),
        (0x23, 0x7F, 0.10416889190673828),
        (0x25, 0x7F, 0.10416889190673828),
        (0x26, 0x7F, 0.10416889190673828),
        (0x27, 0x7F, 0.10416889190673828),
        (0x3D, 0x01, -0.06944561004638672),
        (0x3D, 0x41, -0.06944561004638672),
        (0x3D, 0x4D, -0.06944561004638672),
        (0x3D, 0x4E, -0.06944561004638672),
        (0x3D, 0x59, 0.06944561004638672),
        (0x3D, 0x5A, -0.06944561004638672),
        (0x40, 0x7F, 0.10416889190673828),
        (0x41, 0x7F, 0.173614501953125),
        (0x42, 0x7F, 0.10416889190673828),
        (0x43, 0x3A, -0.06944561004638672),
        (0x43, 0x3B, -0.06944561004638672),
        (0x43, 0x3D, -0.03472328186035156),
        (0x43, 0x7F, 0.10416889190673828),
        (0x44, 0x7F, 0.06944561004638672),
        (0x45, 0x7F, 0.10416889190673828),
        (0x46, 0x3A, -0.13889122009277344),
        (0x46, 0x3B, -0.13889122009277344),
        (0x46, 0x3D, -0.06944561004638672),
        (0x46, 0x7F, 0.10416889190673828),
        (0x47, 0x7F, 0.10416889190673828),
        (0x48, 0x3A, -0.06944561004638672),
        (0x48, 0x3B, -0.06944561004638672),
        (0x48, 0x3D, -0.06944561004638672),
        (0x48, 0x7F, 0.06944561004638672),
        (0x49, 0x7F, 0.13889122009277344),
        (0x4A, 0x3A, -0.13889122009277344),
        (0x4A, 0x3B, -0.13889122009277344),
        (0x4A, 0x3D, -0.06944561004638672),
        (0x4A, 0x7F, 0.20833778381347656),
        (0x4B, 0x3A, -0.06944561004638672),
        (0x4B, 0x3B, -0.06944561004638672),
        (0x4B, 0x3D, -0.06944561004638672),
        (0x4B, 0x7F, 0.06944561004638672),
        (0x4C, 0x7F, 0.03472328186035156),
        (0x4D, 0x3A, -0.06944561004638672),
        (0x4D, 0x3B, -0.06944561004638672),
        (0x4D, 0x3D, -0.06944561004638672),
        (0x4D, 0x7F, 0.10416889190673828),
        (0x4E, 0x3A, -0.06944561004638672),
        (0x4E, 0x3B, -0.06944561004638672),
        (0x4E, 0x3D, -0.10416889190673828),
        (0x4E, 0x7F, 0.10416889190673828),
        (0x4F, 0x7F, 0.10416889190673828),
        (0x50, 0x3A, -0.13889122009277344),
        (0x50, 0x3B, -0.13889122009277344),
        (0x50, 0x3D, -0.06944561004638672),
        (0x50, 0x7F, 0.10416889190673828),
        (0x51, 0x7F, 0.10416889190673828),
        (0x52, 0x7F, 0.10416889190673828),
        (0x53, 0x3A, -0.06944561004638672),
        (0x53, 0x3B, -0.06944561004638672),
        (0x53, 0x3D, -0.06944561004638672),
        (0x53, 0x7F, 0.10416889190673828),
        (0x54, 0x3A, -0.06944561004638672),
        (0x54, 0x3B, -0.06944561004638672),
        (0x54, 0x3D, -0.03472328186035156),
        (0x54, 0x7F, 0.10416889190673828),
        (0x55, 0x3A, -0.13889122009277344),
        (0x55, 0x3B, -0.13889122009277344),
        (0x55, 0x3D, -0.06944561004638672),
        (0x55, 0x7F, 0.03472328186035156),
        (0x56, 0x3A, -0.20833778381347656),
        (0x56, 0x3B, -0.20833778381347656),
        (0x56, 0x3D, -0.13889122009277344),
        (0x57, 0x3A, -0.20833778381347656),
        (0x57, 0x3B, -0.20833778381347656),
        (0x57, 0x3D, -0.13889122009277344),
        (0x58, 0x3A, -0.06944561004638672),
        (0x58, 0x3B, -0.06944561004638672),
        (0x58, 0x3D, -0.10416889190673828),
        (0x58, 0x7F, 0.10416889190673828),
        (0x59, 0x3A, -0.20833778381347656),
        (0x59, 0x3B, -0.20833778381347656),
        (0x59, 0x3D, -0.13889122009277344),
        (0x5A, 0x3A, -0.06944561004638672),
        (0x5A, 0x3B, -0.06944561004638672),
        (0x5A, 0x3D, -0.06944561004638672),
        (0x5A, 0x7F, 0.10416889190673828),
        (0x60, 0x7F, 0.13889122009277344),
        (0x63, 0x7F, 0.06944561004638672),
        (0x64, 0x59, 0.06944561004638672),
        (0x64, 0x5A, -0.06944561004638672),
        (0x64, 0x66, -0.20833778381347656),
        (0x64, 0x6A, -0.13889122009277344),
        (0x64, 0x7F, 0.20833778381347656),
        (0x65, 0x7F, 0.06944561004638672),
        (0x66, 0x3A, -0.06944561004638672),
        (0x66, 0x3B, -0.06944561004638672),
        (0x66, 0x7F, 0.20833778381347656),
        (0x67, 0x7F, 0.03472328186035156),
        (0x68, 0x7F, -0.03472328186035156),
        (0x6A, 0x3A, -0.06944561004638672),
        (0x6A, 0x3B, -0.06944561004638672),
        (0x6C, 0x7F, 0.10416889190673828),
        (0x6F, 0x7F, 0.06944561004638672),
        (0x70, 0x7F, 0.10416889190673828),
        (0x71, 0x7F, 0.10416889190673828),
        (0x72, 0x3A, -0.06944561004638672),
        (0x72, 0x3B, -0.06944561004638672),
        (0x72, 0x7F, 0.06944561004638672),
        (0x73, 0x7F, 0.06944561004638672),
        (0x74, 0x7F, 0.10416889190673828),
        (0x75, 0x7F, 0.03472328186035156),
        (0x76, 0x7F, 0.03472328186035156),
        (0x77, 0x7F, 0.10416889190673828),
        (0x78, 0x7F, 0.03472328186035156),
        (0x79, 0x7F, 0.06944561004638672),
        (0x7A, 0x7F, 0.06944561004638672),
        (0x7B, 0x7F, 0.03472328186035156),
        (0x7C, 0x7F, 0.10416889190673828),
        (0x7D, 0x7F, 0.13889122009277344),
    ],
};

/// `cmmi6.tfm`.
#[rustfmt::skip]
pub static CMMI6: TfmFont = TfmFont {
    name: "cmmi6",
    space: 0.0,
    skew_char: Some(0x7F),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305543899536133, 1.27777099609375],
    chars: &[
        Some(TfmChar { width: 0.742588996887207, height: 0.6833343505859375, depth: 0.0, italic: 0.16203594207763672 }), // 0x00
        Some(TfmChar { width: 1.02777099609375, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.9340667724609375, height: 0.6833343505859375, depth: 0.0, italic: 0.03240680694580078 }), // 0x02
        Some(TfmChar { width: 0.8592529296875, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.907404899597168, height: 0.6833343505859375, depth: 0.0, italic: 0.07407379150390625 }), // 0x04
        Some(TfmChar { width: 0.9995317459106445, height: 0.6833343505859375, depth: 0.0, italic: 0.07546520233154297 }), // 0x05
        Some(TfmChar { width: 0.9516172409057617, height: 0.6833343505859375, depth: 0.0, italic: 0.05347156524658203 }), // 0x06
        Some(TfmChar { width: 0.7361068725585938, height: 0.6833343505859375, depth: 0.0, italic: 0.16203594207763672 }), // 0x07
        Some(TfmChar { width: 0.8333282470703125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.7811965942382812, height: 0.6833343505859375, depth: 0.0, italic: 0.11694622039794922 }), // 0x09
        Some(TfmChar { width: 0.9460020065307617, height: 0.6833343505859375, depth: 0.0, italic: 0.04785919189453125 }), // 0x0A
        Some(TfmChar { width: 0.8045120239257812, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.698033332824707, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.05138683319091797 }), // 0x0C
        Some(TfmChar { width: 0.6519851684570312, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.06481456756591797 }), // 0x0D
        Some(TfmChar { width: 0.5662031173706055, height: 0.6944456100463867, depth: 0.0, italic: 0.03460693359375 }), // 0x0E
        Some(TfmChar { width: 0.5232620239257812, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.5717573165893555, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.05989837646484375 }), // 0x10
        Some(TfmChar { width: 0.643977165222168, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.03588104248046875 }), // 0x11
        Some(TfmChar { width: 0.5902814865112305, height: 0.6944456100463867, depth: 0.0, italic: 0.03240680694580078 }), // 0x12
        Some(TfmChar { width: 0.46643543243408203, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.7256927490234375, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.7361068725585938, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.75, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.6215238571166992, height: 0.4305543899536133, depth: 0.0, italic: 0.06828784942626953 }), // 0x17
        Some(TfmChar { width: 0.5717573165893555, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.02749156951904297 }), // 0x18
        Some(TfmChar { width: 0.7267303466796875, height: 0.4305543899536133, depth: 0.0, italic: 0.03588104248046875 }), // 0x19
        Some(TfmChar { width: 0.639007568359375, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.716547966003418, height: 0.4305543899536133, depth: 0.0, italic: 0.03588104248046875 }), // 0x1B
        Some(TfmChar { width: 0.582056999206543, height: 0.4305543899536133, depth: 0.0, italic: 0.11574077606201172 }), // 0x1C
        Some(TfmChar { width: 0.6898117065429688, height: 0.4305543899536133, depth: 0.0, italic: 0.03587818145751953 }), // 0x1D
        Some(TfmChar { width: 0.7421293258666992, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.7673568725585938, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.8194398880004883, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.03587818145751953 }), // 0x20
        Some(TfmChar { width: 0.7798566818237305, height: 0.4305543899536133, depth: 0.0, italic: 0.03587818145751953 }), // 0x21
        Some(TfmChar { width: 0.5866851806640625, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.7506914138793945, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.0218687057495117, height: 0.4305543899536133, depth: 0.0, italic: 0.03240680694580078 }), // 0x24
        Some(TfmChar { width: 0.639007568359375, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.48784351348876953, height: 0.4305543899536133, depth: 0.09722423553466797, italic: 0.07523059844970703 }), // 0x26
        Some(TfmChar { width: 0.8115692138671875, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.37962818145751953, height: 0.5040283203125, depth: 0.0040283203125, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.37962818145751953, height: 0.5040283203125, depth: 0.0040283203125, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.638885498046875, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.638885498046875, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.638885498046875, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.638885498046875, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.638885498046875, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.638885498046875, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.638885498046875, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.638885498046875, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.638885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.638885498046875, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.638885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.638885498046875, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.37962818145751953, height: 0.12037181854248047, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.37962818145751953, height: 0.12037181854248047, depth: 0.19444561004638672, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.962956428527832, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.638885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.962956428527832, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.638885498046875, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.6586809158325195, height: 0.6944456100463867, depth: 0.0, italic: 0.06481456756591797 }), // 0x40
        Some(TfmChar { width: 0.924067497253418, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.9265565872192383, height: 0.6833343505859375, depth: 0.0, italic: 0.04785919189453125 }), // 0x42
        Some(TfmChar { width: 0.883697509765625, height: 0.6833343505859375, depth: 0.0, italic: 0.0692138671875 }), // 0x43
        Some(TfmChar { width: 0.9982805252075195, height: 0.6833343505859375, depth: 0.0, italic: 0.03240680694580078 }), // 0x44
        Some(TfmChar { width: 0.8997650146484375, height: 0.6833343505859375, depth: 0.0, italic: 0.05347156524658203 }), // 0x45
        Some(TfmChar { width: 0.7749967575073242, height: 0.6833343505859375, depth: 0.0, italic: 0.16203594207763672 }), // 0x46
        Some(TfmChar { width: 0.952911376953125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.9995317459106445, height: 0.6833343505859375, depth: 0.0, italic: 0.07546520233154297 }), // 0x48
        Some(TfmChar { width: 0.5476837158203125, height: 0.6833343505859375, depth: 0.0, italic: 0.07731342315673828 }), // 0x49
        Some(TfmChar { width: 0.6815977096557617, height: 0.6833343505859375, depth: 0.0, italic: 0.09456157684326172 }), // 0x4A
        Some(TfmChar { width: 1.0256881713867188, height: 0.6833343505859375, depth: 0.0, italic: 0.0692138671875 }), // 0x4B
        Some(TfmChar { width: 0.8462905883789062, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.1615676879882812, height: 0.6833343505859375, depth: 0.0, italic: 0.10787200927734375 }), // 0x4D
        Some(TfmChar { width: 0.9671249389648438, height: 0.6833343505859375, depth: 0.0, italic: 0.10787200927734375 }), // 0x4E
        Some(TfmChar { width: 0.9340667724609375, height: 0.6833343505859375, depth: 0.0, italic: 0.03240680694580078 }), // 0x4F
        Some(TfmChar { width: 0.779973030090332, height: 0.6833343505859375, depth: 0.0, italic: 0.16203594207763672 }), // 0x50
        Some(TfmChar { width: 0.9664735794067383, height: 0.6833343505859375, depth: 0.19444561004638672, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.9221296310424805, height: 0.6833343505859375, depth: 0.0, italic: 0.0036745071411132812 }), // 0x52
        Some(TfmChar { width: 0.7567110061645508, height: 0.6833343505859375, depth: 0.0, italic: 0.053009033203125 }), // 0x53
        Some(TfmChar { width: 0.7311296463012695, height: 0.6833343505859375, depth: 0.0, italic: 0.16203594207763672 }), // 0x54
        Some(TfmChar { width: 0.8380966186523438, height: 0.6833343505859375, depth: 0.0, italic: 0.10787200927734375 }), // 0x55
        Some(TfmChar { width: 0.7296266555786133, height: 0.6833343505859375, depth: 0.0, italic: 0.25925731658935547 }), // 0x56
        Some(TfmChar { width: 1.1509199142456055, height: 0.6833343505859375, depth: 0.0, italic: 0.16203594207763672 }), // 0x57
        Some(TfmChar { width: 1.0013837814331055, height: 0.6833343505859375, depth: 0.0, italic: 0.07731342315673828 }), // 0x58
        Some(TfmChar { width: 0.726384162902832, height: 0.6833343505859375, depth: 0.0, italic: 0.25925731658935547 }), // 0x59
        Some(TfmChar { width: 0.8377275466918945, height: 0.6833343505859375, depth: 0.0, italic: 0.0692138671875 }), // 0x5A
        Some(TfmChar { width: 0.5092573165893555, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.5092573165893555, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.5092573165893555, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.2222137451171875, height: 0.35764026641845703, depth: -0.14235973358154297, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.2222137451171875, height: 0.35764026641845703, depth: -0.14235973358154297, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.518519401550293, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.6748838424682617, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.5476865768432617, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.55914306640625, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.6424760818481445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.5890045166015625, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.6006927490234375, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.11458587646484375 }), // 0x66
        Some(TfmChar { width: 0.6077470779418945, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.03588104248046875 }), // 0x67
        Some(TfmChar { width: 0.7256927490234375, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.4456024169921875, height: 0.6625337600708008, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.5115737915039062, height: 0.6625337600708008, depth: 0.19444561004638672, italic: 0.05799579620361328 }), // 0x6A
        Some(TfmChar { width: 0.6608781814575195, height: 0.6944456100463867, depth: 0.0, italic: 0.02083110809326172 }), // 0x6B
        Some(TfmChar { width: 0.40162181854248047, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 1.093745231628418, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.7696733474731445, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.6125001907348633, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.6424760818481445, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.570713996887207, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.03588104248046875 }), // 0x71
        Some(TfmChar { width: 0.579859733581543, height: 0.4305543899536133, depth: 0.0, italic: 0.03240680694580078 }), // 0x72
        Some(TfmChar { width: 0.5844907760620117, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.4768495559692383, height: 0.6150789260864258, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.7372665405273438, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.6249971389770508, height: 0.4305543899536133, depth: 0.0, italic: 0.03587818145751953 }), // 0x76
        Some(TfmChar { width: 0.893223762512207, height: 0.4305543899536133, depth: 0.0, italic: 0.02690887451171875 }), // 0x77
        Some(TfmChar { width: 0.6979141235351562, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.6330976486206055, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.03588104248046875 }), // 0x79
        Some(TfmChar { width: 0.596064567565918, height: 0.4305543899536133, depth: 0.0, italic: 0.03934955596923828 }), // 0x7A
        Some(TfmChar { width: 0.4456024169921875, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.47916698455810547, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.7871503829956055, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.638885498046875, height: 0.7202787399291992, depth: 0.0, italic: 0.15381908416748047 }), // 0x7E
        Some(TfmChar { width: 0.37962818145751953, height: 0.6944456100463867, depth: 0.0, italic: 0.5027008056640625 }), // 0x7F
    ],
    kerns: &[
        (0x00, 0x3A, -0.12962818145751953),
        (0x00, 0x3B, -0.12962818145751953),
        (0x00, 0x3D, -0.06481456756591797),
        (0x00, 0x7F, 0.09722137451171875),
        (0x01, 0x7F, 0.1944427490234375),
        (0x02, 0x7F, 0.09722137451171875),
        (0x03, 0x7F, 0.1944427490234375),
        (0x04, 0x7F, 0.09722137451171875),
        (0x05, 0x3A, -0.06481456756591797),
        (0x05, 0x3B, -0.06481456756591797),
        (0x05, 0x3D, -0.06481456756591797),
        (0x05, 0x7F, 0.06481456756591797),
        (0x06, 0x7F, 0.09722137451171875),
        (0x07, 0x3A, -0.12962818145751953),
        (0x07, 0x3B, -0.12962818145751953),
        (0x07, 0x3D, -0.06481456756591797),
        (0x07, 0x7F, 0.06481456756591797),
        (0x08, 0x7F, 0.09722137451171875),
        (0x09, 0x3A, -0.06481456756591797),
        (0x09, 0x3B, -0.06481456756591797),
        (0x09, 0x3D, -0.06481456756591797),
        (0x09, 0x7F, 0.06481456756591797),
        (0x0A, 0x7F, 0.09722137451171875),
        (0x0B, 0x7F, 0.03240680694580078),
        (0x0C, 0x7F, 0.09722137451171875),
        (0x0E, 0x3A, -0.06481456756591797),
        (0x0E, 0x3B, -0.06481456756591797),
        (0x0E, 0x7F, 0.06481456756591797),
        (0x0F, 0x7F, 0.06481456756591797),
        (0x10, 0x7F, 0.09722137451171875),
        (0x11, 0x7F, 0.06481456756591797),
        (0x12, 0x7F, 0.09722137451171875),
        (0x13, 0x7F, 0.06481456756591797),
        (0x16, 0x7F, 0.03240680694580078),
        (0x17, 0x3A, -0.06481456756591797),
        (0x17, 0x3B, -0.06481456756591797),
        (0x17, 0x7F, 0.03240680694580078),
        (0x18, 0x7F, 0.12962818145751953),
        (0x1A, 0x7F, 0.09722137451171875),
        (0x1B, 0x3A, -0.06481456756591797),
        (0x1B, 0x3B, -0.06481456756591797),
        (0x1C, 0x3A, -0.06481456756591797),
        (0x1C, 0x3B, -0.06481456756591797),
        (0x1C, 0x7F, 0.03240680694580078),
        (0x1D, 0x7F, 0.03240680694580078),
        (0x1E, 0x7F, 0.09722137451171875),
        (0x1F, 0x7F, 0.06481456756591797),
        (0x20, 0x7F, 0.12962818145751953),
        (0x22, 0x7F, 0.09722137451171875),
        (0x23, 0x7F, 0.09722137451171875),
        (0x25, 0x7F, 0.09722137451171875),
        (0x26, 0x7F, 0.09722137451171875),
        (0x27, 0x7F, 0.09722137451171875),
        (0x3D, 0x01, -0.06481456756591797),
        (0x3D, 0x41, -0.06481456756591797),
        (0x3D, 0x4D, -0.06481456756591797),
        (0x3D, 0x4E, -0.06481456756591797),
        (0x3D, 0x59, 0.06481456756591797),
        (0x3D, 0x5A, -0.06481456756591797),
        (0x40, 0x7F, 0.09722137451171875),
        (0x41, 0x7F, 0.16203594207763672),
        (0x42, 0x7F, 0.09722137451171875),
        (0x43, 0x3A, -0.06481456756591797),
        (0x43, 0x3B, -0.06481456756591797),
        (0x43, 0x3D, -0.03240680694580078),
        (0x43, 0x7F, 0.09722137451171875),
        (0x44, 0x7F, 0.06481456756591797),
        (0x45, 0x7F, 0.09722137451171875),
        (0x46, 0x3A, -0.12962818145751953),
        (0x46, 0x3B, -0.12962818145751953),
        (0x46, 0x3D, -0.06481456756591797),
        (0x46, 0x7F, 0.09722137451171875),
        (0x47, 0x7F, 0.09722137451171875),
        (0x48, 0x3A, -0.06481456756591797),
        (0x48, 0x3B, -0.06481456756591797),
        (0x48, 0x3D, -0.06481456756591797),
        (0x48, 0x7F, 0.06481456756591797),
        (0x49, 0x7F, 0.12962818145751953),
        (0x4A, 0x3A, -0.12962818145751953),
        (0x4A, 0x3B, -0.12962818145751953),
        (0x4A, 0x3D, -0.06481456756591797),
        (0x4A, 0x7F, 0.1944427490234375),
        (0x4B, 0x3A, -0.06481456756591797),
        (0x4B, 0x3B, -0.06481456756591797),
        (0x4B, 0x3D, -0.06481456756591797),
        (0x4B, 0x7F, 0.06481456756591797),
        (0x4C, 0x7F, 0.03240680694580078),
        (0x4D, 0x3A, -0.06481456756591797),
        (0x4D, 0x3B, -0.06481456756591797),
        (0x4D, 0x3D, -0.06481456756591797),
        (0x4D, 0x7F, 0.09722137451171875),
        (0x4E, 0x3A, -0.06481456756591797),
        (0x4E, 0x3B, -0.06481456756591797),
        (0x4E, 0x3D, -0.09722137451171875),
        (0x4E, 0x7F, 0.09722137451171875),
        (0x4F, 0x7F, 0.09722137451171875),
        (0x50, 0x3A, -0.12962818145751953),
        (0x50, 0x3B, -0.12962818145751953),
        (0x50, 0x3D, -0.06481456756591797),
        (0x50, 0x7F, 0.09722137451171875),
        (0x51, 0x7F, 0.09722137451171875),
        (0x52, 0x7F, 0.09722137451171875),
        (0x53, 0x3A, -0.06481456756591797),
        (0x53, 0x3B, -0.06481456756591797),
        (0x53, 0x3D, -0.06481456756591797),
        (0x53, 0x7F, 0.09722137451171875),
        (0x54, 0x3A, -0.06481456756591797),
        (0x54, 0x3B, -0.06481456756591797),
        (0x54, 0x3D, -0.03240680694580078),
        (0x54, 0x7F, 0.09722137451171875),
        (0x55, 0x3A, -0.12962818145751953),
        (0x55, 0x3B, -0.12962818145751953),
        (0x55, 0x3D, -0.06481456756591797),
        (0x55, 0x7F, 0.03240680694580078),
        (0x56, 0x3A, -0.1944427490234375),
        (0x56, 0x3B, -0.1944427490234375),
        (0x56, 0x3D, -0.12962818145751953),
        (0x57, 0x3A, -0.1944427490234375),
        (0x57, 0x3B, -0.1944427490234375),
        (0x57, 0x3D, -0.12962818145751953),
        (0x58, 0x3A, -0.06481456756591797),
        (0x58, 0x3B, -0.06481456756591797),
        (0x58, 0x3D, -0.09722137451171875),
        (0x58, 0x7F, 0.09722137451171875),
        (0x59, 0x3A, -0.1944427490234375),
        (0x59, 0x3B, -0.1944427490234375),
        (0x59, 0x3D, -0.12962818145751953),
        (0x5A, 0x3A, -0.06481456756591797),
        (0x5A, 0x3B, -0.06481456756591797),
        (0x5A, 0x3D, -0.06481456756591797),
        (0x5A, 0x7F, 0.09722137451171875),
        (0x60, 0x7F, 0.12962818145751953),
        (0x63, 0x7F, 0.06481456756591797),
        (0x64, 0x59, 0.06481456756591797),
        (0x64, 0x5A, -0.06481456756591797),
        (0x64, 0x66, -0.1944427490234375),
        (0x64, 0x6A, -0.12962818145751953),
        (0x64, 0x7F, 0.1944427490234375),
        (0x65, 0x7F, 0.06481456756591797),
        (0x66, 0x3A, -0.06481456756591797),
        (0x66, 0x3B, -0.06481456756591797),
        (0x66, 0x7F, 0.1944427490234375),
        (0x67, 0x7F, 0.03240680694580078),
        (0x68, 0x7F, -0.03240680694580078),
        (0x6A, 0x3A, -0.06481456756591797),
        (0x6A, 0x3B, -0.06481456756591797),
        (0x6C, 0x7F, 0.09722137451171875),
        (0x6F, 0x7F, 0.06481456756591797),
        (0x70, 0x7F, 0.09722137451171875),
        (0x71, 0x7F, 0.09722137451171875),
        (0x72, 0x3A, -0.06481456756591797),
        (0x72, 0x3B, -0.06481456756591797),
        (0x72, 0x7F, 0.06481456756591797),
        (0x73, 0x7F, 0.06481456756591797),
        (0x74, 0x7F, 0.09722137451171875),
        (0x75, 0x7F, 0.03240680694580078),
        (0x76, 0x7F, 0.03240680694580078),
        (0x77, 0x7F, 0.09722137451171875),
        (0x78, 0x7F, 0.03240680694580078),
        (0x79, 0x7F, 0.06481456756591797),
        (0x7A, 0x7F, 0.06481456756591797),
        (0x7B, 0x7F, 0.03240680694580078),
        (0x7C, 0x7F, 0.09722137451171875),
        (0x7D, 0x7F, 0.12962818145751953),
    ],
};

/// `cmmi8.tfm`.
#[rustfmt::skip]
pub static CMMI8: TfmFont = TfmFont {
    name: "cmmi8",
    space: 0.0,
    skew_char: Some(0x7F),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305553436279297, 1.0625152587890625],
    chars: &[
        Some(TfmChar { width: 0.6428890228271484, height: 0.6833324432373047, depth: 0.0, italic: 0.14757156372070312 }), // 0x00
        Some(TfmChar { width: 0.8854293823242188, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.8061904907226562, height: 0.6833324432373047, depth: 0.0, italic: 0.029514312744140625 }), // 0x02
        Some(TfmChar { width: 0.73681640625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.7834300994873047, height: 0.6833324432373047, depth: 0.0, italic: 0.07508659362792969 }), // 0x04
        Some(TfmChar { width: 0.8728408813476562, height: 0.6833324432373047, depth: 0.0, italic: 0.076995849609375 }), // 0x05
        Some(TfmChar { width: 0.8234481811523438, height: 0.6833324432373047, depth: 0.0, italic: 0.0560760498046875 }), // 0x06
        Some(TfmChar { width: 0.6198005676269531, height: 0.6833324432373047, depth: 0.0, italic: 0.14757156372070312 }), // 0x07
        Some(TfmChar { width: 0.708343505859375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.6547660827636719, height: 0.6833324432373047, depth: 0.0, italic: 0.11260604858398438 }), // 0x09
        Some(TfmChar { width: 0.8166770935058594, height: 0.6833324432373047, depth: 0.0, italic: 0.049304962158203125 }), // 0x0A
        Some(TfmChar { width: 0.6824169158935547, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.5961875915527344, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.051387786865234375 }), // 0x0C
        Some(TfmChar { width: 0.5472526550292969, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.05902862548828125 }), // 0x0D
        Some(TfmChar { width: 0.47014617919921875, height: 0.6944446563720703, depth: 0.0, italic: 0.03749847412109375 }), // 0x0E
        Some(TfmChar { width: 0.4295196533203125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.4670219421386719, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.06857490539550781 }), // 0x10
        Some(TfmChar { width: 0.5331668853759766, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x11
        Some(TfmChar { width: 0.4956645965576172, height: 0.6944446563720703, depth: 0.0, italic: 0.029514312744140625 }), // 0x12
        Some(TfmChar { width: 0.3761615753173828, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.6122760772705078, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.6198005676269531, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6391849517822266, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.5222873687744141, height: 0.4305553436279297, depth: 0.0, italic: 0.06539344787597656 }), // 0x17
        Some(TfmChar { width: 0.4670219421386719, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.03906059265136719 }), // 0x18
        Some(TfmChar { width: 0.6101360321044922, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x19
        Some(TfmChar { width: 0.5441036224365234, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.6071853637695312, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x1B
        Some(TfmChar { width: 0.47153472900390625, height: 0.4305553436279297, depth: 0.0, italic: 0.11545181274414062 }), // 0x1C
        Some(TfmChar { width: 0.5763969421386719, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x1D
        Some(TfmChar { width: 0.6316070556640625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.65972900390625, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.6944541931152344, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x20
        Some(TfmChar { width: 0.6606578826904297, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x21
        Some(TfmChar { width: 0.490631103515625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.6320686340332031, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.8821296691894531, height: 0.4305553436279297, depth: 0.0, italic: 0.029514312744140625 }), // 0x24
        Some(TfmChar { width: 0.5441036224365234, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.38889503479003906, height: 0.4305553436279297, depth: 0.09722328186035156, italic: 0.078125 }), // 0x26
        Some(TfmChar { width: 0.6923713684082031, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.29514312744140625, height: 0.4834728240966797, depth: -0.016527175903320312, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.29514312744140625, height: 0.4834728240966797, depth: -0.016527175903320312, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.5312576293945312, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.5312576293945312, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.29514312744140625, height: 0.11111068725585938, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.29514312744140625, height: 0.11111068725585938, depth: 0.1944446563720703, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.8264007568359375, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.8264007568359375, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.5312576293945312, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.5597286224365234, height: 0.6944446563720703, depth: 0.0, italic: 0.05902862548828125 }), // 0x40
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.8013992309570312, height: 0.6833324432373047, depth: 0.0, italic: 0.049304962158203125 }), // 0x42
        Some(TfmChar { width: 0.7572669982910156, height: 0.6833324432373047, depth: 0.0, italic: 0.07065963745117188 }), // 0x43
        Some(TfmChar { width: 0.8716773986816406, height: 0.6833324432373047, depth: 0.0, italic: 0.029514312744140625 }), // 0x44
        Some(TfmChar { width: 0.778656005859375, height: 0.6833324432373047, depth: 0.0, italic: 0.0560760498046875 }), // 0x45
        Some(TfmChar { width: 0.6724033355712891, height: 0.6833324432373047, depth: 0.0, italic: 0.14757156372070312 }), // 0x46
        Some(TfmChar { width: 0.8279266357421875, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.8728408813476562, height: 0.6833324432373047, depth: 0.0, italic: 0.076995849609375 }), // 0x48
        Some(TfmChar { width: 0.46068382263183594, height: 0.6833324432373047, depth: 0.0, italic: 0.07803726196289062 }), // 0x49
        Some(TfmChar { width: 0.5803890228271484, height: 0.6833324432373047, depth: 0.0, italic: 0.09392356872558594 }), // 0x4A
        Some(TfmChar { width: 0.8960189819335938, height: 0.6833324432373047, depth: 0.0, italic: 0.07065963745117188 }), // 0x4B
        Some(TfmChar { width: 0.7225799560546875, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.0204124450683594, height: 0.6833324432373047, depth: 0.0, italic: 0.10651016235351562 }), // 0x4D
        Some(TfmChar { width: 0.8433265686035156, height: 0.6833324432373047, depth: 0.0, italic: 0.10651016235351562 }), // 0x4E
        Some(TfmChar { width: 0.8061904907226562, height: 0.6833324432373047, depth: 0.0, italic: 0.029514312744140625 }), // 0x4F
        Some(TfmChar { width: 0.6736183166503906, height: 0.6833324432373047, depth: 0.0, italic: 0.14757156372070312 }), // 0x50
        Some(TfmChar { width: 0.8357048034667969, height: 0.6833324432373047, depth: 0.1944446563720703, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.8002262115478516, height: 0.6833324432373047, depth: 0.0, italic: 0.006206512451171875 }), // 0x52
        Some(TfmChar { width: 0.6461887359619141, height: 0.6833324432373047, depth: 0.0, italic: 0.05590248107910156 }), // 0x53
        Some(TfmChar { width: 0.6185855865478516, height: 0.6833324432373047, depth: 0.0, italic: 0.14757156372070312 }), // 0x54
        Some(TfmChar { width: 0.71881103515625, height: 0.6833324432373047, depth: 0.0, italic: 0.10651016235351562 }), // 0x55
        Some(TfmChar { width: 0.6187591552734375, height: 0.6833324432373047, depth: 0.0, italic: 0.236114501953125 }), // 0x56
        Some(TfmChar { width: 1.0024452209472656, height: 0.6833324432373047, depth: 0.0, italic: 0.14757156372070312 }), // 0x57
        Some(TfmChar { width: 0.8738842010498047, height: 0.6833324432373047, depth: 0.0, italic: 0.07803726196289062 }), // 0x58
        Some(TfmChar { width: 0.6158084869384766, height: 0.6833324432373047, depth: 0.0, italic: 0.236114501953125 }), // 0x59
        Some(TfmChar { width: 0.7199745178222656, height: 0.6833324432373047, depth: 0.0, italic: 0.07065963745117188 }), // 0x5A
        Some(TfmChar { width: 0.41320037841796875, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.41320037841796875, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.41320037841796875, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.0625152587890625, height: 0.3576393127441406, depth: -0.14236068725585938, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.0625152587890625, height: 0.3576393127441406, depth: -0.14236068725585938, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.4340324401855469, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.5643596649169922, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.45452117919921875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.4601917266845703, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.5467071533203125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.492889404296875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.5104217529296875, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.11024284362792969 }), // 0x66
        Some(TfmChar { width: 0.5056209564208984, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x67
        Some(TfmChar { width: 0.6122760772705078, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.3617439270019531, height: 0.6610870361328125, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.4296913146972656, height: 0.6610870361328125, depth: 0.1944446563720703, italic: 0.0576324462890625 }), // 0x6A
        Some(TfmChar { width: 0.5532474517822266, height: 0.6944446563720703, depth: 0.0, italic: 0.026620864868164062 }), // 0x6B
        Some(TfmChar { width: 0.31713294982910156, height: 0.6944446563720703, depth: 0.0, italic: 0.011861801147460938 }), // 0x6C
        Some(TfmChar { width: 0.9398269653320312, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.644683837890625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.5135498046875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5348453521728516, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.47437095642089844, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x71
        Some(TfmChar { width: 0.479461669921875, height: 0.4305553436279297, depth: 0.0, italic: 0.029514312744140625 }), // 0x72
        Some(TfmChar { width: 0.49132537841796875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.3836860656738281, height: 0.6150798797607422, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.6151695251464844, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.5173683166503906, height: 0.4305553436279297, depth: 0.0, italic: 0.03587913513183594 }), // 0x76
        Some(TfmChar { width: 0.7624530792236328, height: 0.4305553436279297, depth: 0.0, italic: 0.02690887451171875 }), // 0x77
        Some(TfmChar { width: 0.5980968475341797, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5251808166503906, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03587913513183594 }), // 0x79
        Some(TfmChar { width: 0.4942188262939453, height: 0.4305553436279297, depth: 0.0, italic: 0.04224586486816406 }), // 0x7A
        Some(TfmChar { width: 0.34954071044921875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.400177001953125, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.6734466552734375, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.5312576293945312, height: 0.7169456481933594, depth: 0.0, italic: 0.15382003784179688 }), // 0x7E
        Some(TfmChar { width: 0.29514312744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.418212890625 }), // 0x7F
    ],
    kerns: &[
        (0x00, 0x3A, -0.1180572509765625),
        (0x00, 0x3B, -0.1180572509765625),
        (0x00, 0x3D, -0.05902862548828125),
        (0x00, 0x7F, 0.08854293823242188),
        (0x01, 0x7F, 0.17708587646484375),
        (0x02, 0x7F, 0.08854293823242188),
        (0x03, 0x7F, 0.17708587646484375),
        (0x04, 0x7F, 0.08854293823242188),
        (0x05, 0x3A, -0.05902862548828125),
        (0x05, 0x3B, -0.05902862548828125),
        (0x05, 0x3D, -0.05902862548828125),
        (0x05, 0x7F, 0.05902862548828125),
        (0x06, 0x7F, 0.08854293823242188),
        (0x07, 0x3A, -0.1180572509765625),
        (0x07, 0x3B, -0.1180572509765625),
        (0x07, 0x3D, -0.05902862548828125),
        (0x07, 0x7F, 0.05902862548828125),
        (0x08, 0x7F, 0.08854293823242188),
        (0x09, 0x3A, -0.05902862548828125),
        (0x09, 0x3B, -0.05902862548828125),
        (0x09, 0x3D, -0.05902862548828125),
        (0x09, 0x7F, 0.05902862548828125),
        (0x0A, 0x7F, 0.08854293823242188),
        (0x0B, 0x7F, 0.029514312744140625),
        (0x0C, 0x7F, 0.08854293823242188),
        (0x0E, 0x3A, -0.05902862548828125),
        (0x0E, 0x3B, -0.05902862548828125),
        (0x0E, 0x7F, 0.05902862548828125),
        (0x0F, 0x7F, 0.05902862548828125),
        (0x10, 0x7F, 0.08854293823242188),
        (0x11, 0x7F, 0.05902862548828125),
        (0x12, 0x7F, 0.08854293823242188),
        (0x13, 0x7F, 0.05902862548828125),
        (0x16, 0x7F, 0.029514312744140625),
        (0x17, 0x3A, -0.05902862548828125),
        (0x17, 0x3B, -0.05902862548828125),
        (0x17, 0x7F, 0.029514312744140625),
        (0x18, 0x7F, 0.1180572509765625),
        (0x1A, 0x7F, 0.08854293823242188),
        (0x1B, 0x3A, -0.05902862548828125),
        (0x1B, 0x3B, -0.05902862548828125),
        (0x1C, 0x3A, -0.05902862548828125),
        (0x1C, 0x3B, -0.05902862548828125),
        (0x1C, 0x7F, 0.029514312744140625),
        (0x1D, 0x7F, 0.029514312744140625),
        (0x1E, 0x7F, 0.08854293823242188),
        (0x1F, 0x7F, 0.05902862548828125),
        (0x20, 0x7F, 0.1180572509765625),
        (0x22, 0x7F, 0.08854293823242188),
        (0x23, 0x7F, 0.08854293823242188),
        (0x25, 0x7F, 0.08854293823242188),
        (0x26, 0x7F, 0.08854293823242188),
        (0x27, 0x7F, 0.08854293823242188),
        (0x3D, 0x01, -0.05902862548828125),
        (0x3D, 0x41, -0.05902862548828125),
        (0x3D, 0x4D, -0.05902862548828125),
        (0x3D, 0x4E, -0.05902862548828125),
        (0x3D, 0x59, 0.05902862548828125),
        (0x3D, 0x5A, -0.05902862548828125),
        (0x40, 0x7F, 0.08854293823242188),
        (0x41, 0x7F, 0.14757156372070312),
        (0x42, 0x7F, 0.08854293823242188),
        (0x43, 0x3A, -0.05902862548828125),
        (0x43, 0x3B, -0.05902862548828125),
        (0x43, 0x3D, -0.029514312744140625),
        (0x43, 0x7F, 0.08854293823242188),
        (0x44, 0x7F, 0.05902862548828125),
        (0x45, 0x7F, 0.08854293823242188),
        (0x46, 0x3A, -0.1180572509765625),
        (0x46, 0x3B, -0.1180572509765625),
        (0x46, 0x3D, -0.05902862548828125),
        (0x46, 0x7F, 0.08854293823242188),
        (0x47, 0x7F, 0.08854293823242188),
        (0x48, 0x3A, -0.05902862548828125),
        (0x48, 0x3B, -0.05902862548828125),
        (0x48, 0x3D, -0.05902862548828125),
        (0x48, 0x7F, 0.05902862548828125),
        (0x49, 0x7F, 0.1180572509765625),
        (0x4A, 0x3A, -0.1180572509765625),
        (0x4A, 0x3B, -0.1180572509765625),
        (0x4A, 0x3D, -0.05902862548828125),
        (0x4A, 0x7F, 0.17708587646484375),
        (0x4B, 0x3A, -0.05902862548828125),
        (0x4B, 0x3B, -0.05902862548828125),
        (0x4B, 0x3D, -0.05902862548828125),
        (0x4B, 0x7F, 0.05902862548828125),
        (0x4C, 0x7F, 0.029514312744140625),
        (0x4D, 0x3A, -0.05902862548828125),
        (0x4D, 0x3B, -0.05902862548828125),
        (0x4D, 0x3D, -0.05902862548828125),
        (0x4D, 0x7F, 0.08854293823242188),
        (0x4E, 0x3A, -0.05902862548828125),
        (0x4E, 0x3B, -0.05902862548828125),
        (0x4E, 0x3D, -0.08854293823242188),
        (0x4E, 0x7F, 0.08854293823242188),
        (0x4F, 0x7F, 0.08854293823242188),
        (0x50, 0x3A, -0.1180572509765625),
        (0x50, 0x3B, -0.1180572509765625),
        (0x50, 0x3D, -0.05902862548828125),
        (0x50, 0x7F, 0.08854293823242188),
        (0x51, 0x7F, 0.08854293823242188),
        (0x52, 0x7F, 0.08854293823242188),
        (0x53, 0x3A, -0.05902862548828125),
        (0x53, 0x3B, -0.05902862548828125),
        (0x53, 0x3D, -0.05902862548828125),
        (0x53, 0x7F, 0.08854293823242188),
        (0x54, 0x3A, -0.05902862548828125),
        (0x54, 0x3B, -0.05902862548828125),
        (0x54, 0x3D, -0.029514312744140625),
        (0x54, 0x7F, 0.08854293823242188),
        (0x55, 0x3A, -0.1180572509765625),
        (0x55, 0x3B, -0.1180572509765625),
        (0x55, 0x3D, -0.05902862548828125),
        (0x55, 0x7F, 0.029514312744140625),
        (0x56, 0x3A, -0.17708587646484375),
        (0x56, 0x3B, -0.17708587646484375),
        (0x56, 0x3D, -0.1180572509765625),
        (0x57, 0x3A, -0.17708587646484375),
        (0x57, 0x3B, -0.17708587646484375),
        (0x57, 0x3D, -0.1180572509765625),
        (0x58, 0x3A, -0.05902862548828125),
        (0x58, 0x3B, -0.05902862548828125),
        (0x58, 0x3D, -0.08854293823242188),
        (0x58, 0x7F, 0.08854293823242188),
        (0x59, 0x3A, -0.17708587646484375),
        (0x59, 0x3B, -0.17708587646484375),
        (0x59, 0x3D, -0.1180572509765625),
        (0x5A, 0x3A, -0.05902862548828125),
        (0x5A, 0x3B, -0.05902862548828125),
        (0x5A, 0x3D, -0.05902862548828125),
        (0x5A, 0x7F, 0.08854293823242188),
        (0x60, 0x7F, 0.1180572509765625),
        (0x63, 0x7F, 0.05902862548828125),
        (0x64, 0x59, 0.05902862548828125),
        (0x64, 0x5A, -0.05902862548828125),
        (0x64, 0x66, -0.17708587646484375),
        (0x64, 0x6A, -0.1180572509765625),
        (0x64, 0x7F, 0.17708587646484375),
        (0x65, 0x7F, 0.05902862548828125),
        (0x66, 0x3A, -0.05902862548828125),
        (0x66, 0x3B, -0.05902862548828125),
        (0x66, 0x7F, 0.17708587646484375),
        (0x67, 0x7F, 0.029514312744140625),
        (0x68, 0x7F, -0.029514312744140625),
        (0x6A, 0x3A, -0.05902862548828125),
        (0x6A, 0x3B, -0.05902862548828125),
        (0x6C, 0x7F, 0.08854293823242188),
        (0x6F, 0x7F, 0.05902862548828125),
        (0x70, 0x7F, 0.08854293823242188),
        (0x71, 0x7F, 0.08854293823242188),
        (0x72, 0x3A, -0.05902862548828125),
        (0x72, 0x3B, -0.05902862548828125),
        (0x72, 0x7F, 0.05902862548828125),
        (0x73, 0x7F, 0.05902862548828125),
        (0x74, 0x7F, 0.08854293823242188),
        (0x75, 0x7F, 0.029514312744140625),
        (0x76, 0x7F, 0.029514312744140625),
        (0x77, 0x7F, 0.08854293823242188),
        (0x78, 0x7F, 0.029514312744140625),
        (0x79, 0x7F, 0.05902862548828125),
        (0x7A, 0x7F, 0.05902862548828125),
        (0x7B, 0x7F, 0.029514312744140625),
        (0x7C, 0x7F, 0.08854293823242188),
        (0x7D, 0x7F, 0.1180572509765625),
    ],
};

/// `cmmi9.tfm`.
#[rustfmt::skip]
pub static CMMI9: TfmFont = TfmFont {
    name: "cmmi9",
    space: 0.0,
    skew_char: Some(0x7F),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305553436279297, 1.02777099609375],
    chars: &[
        Some(TfmChar { width: 0.6276969909667969, height: 0.6833324432373047, depth: 0.0, italic: 0.1427459716796875 }), // 0x00
        Some(TfmChar { width: 0.856475830078125, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.7820625305175781, height: 0.6833324432373047, depth: 0.0, italic: 0.0285491943359375 }), // 0x02
        Some(TfmChar { width: 0.7135753631591797, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.7606048583984375, height: 0.6833324432373047, depth: 0.0, italic: 0.0754241943359375 }), // 0x04
        Some(TfmChar { width: 0.8501501083374023, height: 0.6833324432373047, depth: 0.0, italic: 0.07947635650634766 }), // 0x05
        Some(TfmChar { width: 0.799224853515625, height: 0.6833324432373047, depth: 0.0, italic: 0.05694389343261719 }), // 0x06
        Some(TfmChar { width: 0.5995330810546875, height: 0.6833324432373047, depth: 0.0, italic: 0.1427459716796875 }), // 0x07
        Some(TfmChar { width: 0.6851806640625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.6311187744140625, height: 0.6833324432373047, depth: 0.0, italic: 0.1111602783203125 }), // 0x09
        Some(TfmChar { width: 0.7920665740966797, height: 0.6833324432373047, depth: 0.0, italic: 0.04978752136230469 }), // 0x0A
        Some(TfmChar { width: 0.6586761474609375, height: 0.4305553436279297, depth: 0.0, italic: 0.0007715225219726562 }), // 0x0B
        Some(TfmChar { width: 0.5792036056518555, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.05215930938720703 }), // 0x0C
        Some(TfmChar { width: 0.5308446884155273, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.057098388671875 }), // 0x0D
        Some(TfmChar { width: 0.45586109161376953, height: 0.6944446563720703, depth: 0.0, italic: 0.037693023681640625 }), // 0x0E
        Some(TfmChar { width: 0.4163932800292969, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.45061397552490234, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.07147026062011719 }), // 0x10
        Some(TfmChar { width: 0.5131921768188477, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x11
        Some(TfmChar { width: 0.4810943603515625, height: 0.6944446563720703, depth: 0.0, italic: 0.0285491943359375 }), // 0x12
        Some(TfmChar { width: 0.36381053924560547, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5922040939331055, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5995330810546875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6192102432250977, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.5069408416748047, height: 0.4305553436279297, depth: 0.0, italic: 0.06442928314208984 }), // 0x17
        Some(TfmChar { width: 0.45061397552490234, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.04292106628417969 }), // 0x18
        Some(TfmChar { width: 0.5882282257080078, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x19
        Some(TfmChar { width: 0.5294361114501953, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.5876884460449219, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x1B
        Some(TfmChar { width: 0.4524269104003906, height: 0.4305553436279297, depth: 0.0, italic: 0.11458301544189453 }), // 0x1C
        Some(TfmChar { width: 0.5563240051269531, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x1D
        Some(TfmChar { width: 0.611724853515625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.6408147811889648, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.6705207824707031, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x20
        Some(TfmChar { width: 0.6394243240356445, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x21
        Some(TfmChar { width: 0.47711849212646484, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.6094884872436523, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.8525018692016602, height: 0.4305553436279297, depth: 0.0, italic: 0.0285491943359375 }), // 0x24
        Some(TfmChar { width: 0.5294361114501953, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.3744182586669922, height: 0.4305553436279297, depth: 0.09722232818603516, italic: 0.07909011840820312 }), // 0x26
        Some(TfmChar { width: 0.671137809753418, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.285491943359375, height: 0.47347164154052734, depth: -0.026528358459472656, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.285491943359375, height: 0.47347164154052734, depth: -0.026528358459472656, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.513885498046875, height: 0.46527767181396484, depth: -0.034722328186035156, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.513885498046875, height: 0.46527767181396484, depth: -0.034722328186035156, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.285491943359375, height: 0.10802555084228516, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.285491943359375, height: 0.10802555084228516, depth: 0.1944446563720703, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.79937744140625, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.79937744140625, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.513885498046875, height: 0.46527767181396484, depth: -0.034722328186035156, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.5437078475952148, height: 0.6944446563720703, depth: 0.0, italic: 0.057098388671875 }), // 0x40
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.777714729309082, height: 0.6833324432373047, depth: 0.0, italic: 0.04978752136230469 }), // 0x42
        Some(TfmChar { width: 0.7336225509643555, height: 0.6833324432373047, depth: 0.0, italic: 0.07114124298095703 }), // 0x43
        Some(TfmChar { width: 0.8475103378295898, height: 0.6833324432373047, depth: 0.0, italic: 0.0285491943359375 }), // 0x44
        Some(TfmChar { width: 0.7563238143920898, height: 0.6833324432373047, depth: 0.0, italic: 0.05694389343261719 }), // 0x45
        Some(TfmChar { width: 0.6562461853027344, height: 0.6833324432373047, depth: 0.0, italic: 0.1427459716796875 }), // 0x46
        Some(TfmChar { width: 0.8047637939453125, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.8501501083374023, height: 0.6833324432373047, depth: 0.0, italic: 0.07947635650634766 }), // 0x48
        Some(TfmChar { width: 0.4492645263671875, height: 0.6833324432373047, depth: 0.0, italic: 0.07827949523925781 }), // 0x49
        Some(TfmChar { width: 0.566279411315918, height: 0.6833324432373047, depth: 0.0, italic: 0.09529304504394531 }), // 0x4A
        Some(TfmChar { width: 0.870366096496582, height: 0.6833324432373047, depth: 0.0, italic: 0.07114124298095703 }), // 0x4B
        Some(TfmChar { width: 0.6993780136108398, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.9928960800170898, height: 0.6833324432373047, depth: 0.0, italic: 0.10802555084228516 }), // 0x4D
        Some(TfmChar { width: 0.8216009140014648, height: 0.6833324432373047, depth: 0.0, italic: 0.10802555084228516 }), // 0x4E
        Some(TfmChar { width: 0.7820625305175781, height: 0.6833324432373047, depth: 0.0, italic: 0.0285491943359375 }), // 0x4F
        Some(TfmChar { width: 0.6562080383300781, height: 0.6833324432373047, depth: 0.0, italic: 0.1427459716796875 }), // 0x50
        Some(TfmChar { width: 0.8106117248535156, height: 0.6833324432373047, depth: 0.1944446563720703, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7776288986206055, height: 0.6833324432373047, depth: 0.0, italic: 0.007049560546875 }), // 0x52
        Some(TfmChar { width: 0.6278514862060547, height: 0.6833324432373047, depth: 0.0, italic: 0.05686759948730469 }), // 0x53
        Some(TfmChar { width: 0.5995721817016602, height: 0.6833324432373047, depth: 0.0, italic: 0.1427459716796875 }), // 0x54
        Some(TfmChar { width: 0.6990547180175781, height: 0.6833324432373047, depth: 0.0, italic: 0.10802555084228516 }), // 0x55
        Some(TfmChar { width: 0.5993804931640625, height: 0.6833324432373047, depth: 0.0, italic: 0.2283935546875 }), // 0x56
        Some(TfmChar { width: 0.97052001953125, height: 0.6833324432373047, depth: 0.0, italic: 0.1427459716796875 }), // 0x57
        Some(TfmChar { width: 0.8489532470703125, height: 0.6833324432373047, depth: 0.0, italic: 0.07827949523925781 }), // 0x58
        Some(TfmChar { width: 0.5965232849121094, height: 0.6833324432373047, depth: 0.0, italic: 0.2283935546875 }), // 0x59
        Some(TfmChar { width: 0.6992254257202148, height: 0.6833324432373047, depth: 0.0, italic: 0.07114124298095703 }), // 0x5A
        Some(TfmChar { width: 0.399688720703125, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.399688720703125, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.399688720703125, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.02777099609375, height: 0.3576393127441406, depth: -0.14236068725585938, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.02777099609375, height: 0.3576393127441406, depth: -0.14236068725585938, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.4243812561035156, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.5444793701171875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.4404296875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.4449424743652344, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.5325202941894531, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.477813720703125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.49884033203125, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.10879707336425781 }), // 0x66
        Some(TfmChar { width: 0.4900798797607422, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x67
        Some(TfmChar { width: 0.5922040939331055, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.3516883850097656, height: 0.6598339080810547, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.4201374053955078, height: 0.6598339080810547, depth: 0.1944446563720703, italic: 0.057320594787597656 }), // 0x6A
        Some(TfmChar { width: 0.5351057052612305, height: 0.6944446563720703, depth: 0.0, italic: 0.029320716857910156 }), // 0x6B
        Some(TfmChar { width: 0.30671215057373047, height: 0.6944446563720703, depth: 0.0, italic: 0.016590118408203125 }), // 0x6C
        Some(TfmChar { width: 0.9054737091064453, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.6199817657470703, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.497528076171875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5159320831298828, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.4592170715332031, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x71
        Some(TfmChar { width: 0.4637336730957031, height: 0.4305553436279297, depth: 0.0, italic: 0.0285491943359375 }), // 0x72
        Some(TfmChar { width: 0.47877979278564453, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.3711395263671875, height: 0.6150798797607422, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.5914325714111328, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.4992256164550781, height: 0.4305553436279297, depth: 0.0, italic: 0.035880088806152344 }), // 0x76
        Some(TfmChar { width: 0.7365894317626953, height: 0.4305553436279297, depth: 0.0, italic: 0.026909828186035156 }), // 0x77
        Some(TfmChar { width: 0.5825605392456055, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.506169319152832, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.035880088806152344 }), // 0x79
        Some(TfmChar { width: 0.47800731658935547, height: 0.4305553436279297, depth: 0.0, italic: 0.043209075927734375 }), // 0x7A
        Some(TfmChar { width: 0.3344898223876953, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.3915882110595703, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.6532764434814453, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.513885498046875, height: 0.7155561447143555, depth: 0.0, italic: 0.15381908416748047 }), // 0x7E
        Some(TfmChar { width: 0.285491943359375, height: 0.6944446563720703, depth: 0.0, italic: 0.4077911376953125 }), // 0x7F
    ],
    kerns: &[
        (0x00, 0x3A, -0.11419677734375),
        (0x00, 0x3B, -0.11419677734375),
        (0x00, 0x3D, -0.057098388671875),
        (0x00, 0x7F, 0.0856475830078125),
        (0x01, 0x7F, 0.171295166015625),
        (0x02, 0x7F, 0.0856475830078125),
        (0x03, 0x7F, 0.171295166015625),
        (0x04, 0x7F, 0.0856475830078125),
        (0x05, 0x3A, -0.057098388671875),
        (0x05, 0x3B, -0.057098388671875),
        (0x05, 0x3D, -0.057098388671875),
        (0x05, 0x7F, 0.057098388671875),
        (0x06, 0x7F, 0.0856475830078125),
        (0x07, 0x3A, -0.11419677734375),
        (0x07, 0x3B, -0.11419677734375),
        (0x07, 0x3D, -0.057098388671875),
        (0x07, 0x7F, 0.057098388671875),
        (0x08, 0x7F, 0.0856475830078125),
        (0x09, 0x3A, -0.057098388671875),
        (0x09, 0x3B, -0.057098388671875),
        (0x09, 0x3D, -0.057098388671875),
        (0x09, 0x7F, 0.057098388671875),
        (0x0A, 0x7F, 0.0856475830078125),
        (0x0B, 0x7F, 0.0285491943359375),
        (0x0C, 0x7F, 0.0856475830078125),
        (0x0E, 0x3A, -0.057098388671875),
        (0x0E, 0x3B, -0.057098388671875),
        (0x0E, 0x7F, 0.057098388671875),
        (0x0F, 0x7F, 0.057098388671875),
        (0x10, 0x7F, 0.0856475830078125),
        (0x11, 0x7F, 0.057098388671875),
        (0x12, 0x7F, 0.0856475830078125),
        (0x13, 0x7F, 0.057098388671875),
        (0x16, 0x7F, 0.0285491943359375),
        (0x17, 0x3A, -0.057098388671875),
        (0x17, 0x3B, -0.057098388671875),
        (0x17, 0x7F, 0.0285491943359375),
        (0x18, 0x7F, 0.11419677734375),
        (0x1A, 0x7F, 0.0856475830078125),
        (0x1B, 0x3A, -0.057098388671875),
        (0x1B, 0x3B, -0.057098388671875),
        (0x1C, 0x3A, -0.057098388671875),
        (0x1C, 0x3B, -0.057098388671875),
        (0x1C, 0x7F, 0.0285491943359375),
        (0x1D, 0x7F, 0.0285491943359375),
        (0x1E, 0x7F, 0.0856475830078125),
        (0x1F, 0x7F, 0.057098388671875),
        (0x20, 0x7F, 0.11419677734375),
        (0x22, 0x7F, 0.0856475830078125),
        (0x23, 0x7F, 0.0856475830078125),
        (0x25, 0x7F, 0.0856475830078125),
        (0x26, 0x7F, 0.0856475830078125),
        (0x27, 0x7F, 0.0856475830078125),
        (0x3D, 0x01, -0.057098388671875),
        (0x3D, 0x41, -0.057098388671875),
        (0x3D, 0x4D, -0.057098388671875),
        (0x3D, 0x4E, -0.057098388671875),
        (0x3D, 0x59, 0.057098388671875),
        (0x3D, 0x5A, -0.057098388671875),
        (0x40, 0x7F, 0.0856475830078125),
        (0x41, 0x7F, 0.1427459716796875),
        (0x42, 0x7F, 0.0856475830078125),
        (0x43, 0x3A, -0.057098388671875),
        (0x43, 0x3B, -0.057098388671875),
        (0x43, 0x3D, -0.0285491943359375),
        (0x43, 0x7F, 0.0856475830078125),
        (0x44, 0x7F, 0.057098388671875),
        (0x45, 0x7F, 0.0856475830078125),
        (0x46, 0x3A, -0.11419677734375),
        (0x46, 0x3B, -0.11419677734375),
        (0x46, 0x3D, -0.057098388671875),
        (0x46, 0x7F, 0.0856475830078125),
        (0x47, 0x7F, 0.0856475830078125),
        (0x48, 0x3A, -0.057098388671875),
        (0x48, 0x3B, -0.057098388671875),
        (0x48, 0x3D, -0.057098388671875),
        (0x48, 0x7F, 0.057098388671875),
        (0x49, 0x7F, 0.11419677734375),
        (0x4A, 0x3A, -0.11419677734375),
        (0x4A, 0x3B, -0.11419677734375),
        (0x4A, 0x3D, -0.057098388671875),
        (0x4A, 0x7F, 0.171295166015625),
        (0x4B, 0x3A, -0.057098388671875),
        (0x4B, 0x3B, -0.057098388671875),
        (0x4B, 0x3D, -0.057098388671875),
        (0x4B, 0x7F, 0.057098388671875),
        (0x4C, 0x7F, 0.0285491943359375),
        (0x4D, 0x3A, -0.057098388671875),
        (0x4D, 0x3B, -0.057098388671875),
        (0x4D, 0x3D, -0.057098388671875),
        (0x4D, 0x7F, 0.0856475830078125),
        (0x4E, 0x3A, -0.057098388671875),
        (0x4E, 0x3B, -0.057098388671875),
        (0x4E, 0x3D, -0.0856475830078125),
        (0x4E, 0x7F, 0.0856475830078125),
        (0x4F, 0x7F, 0.0856475830078125),
        (0x50, 0x3A, -0.11419677734375),
        (0x50, 0x3B, -0.11419677734375),
        (0x50, 0x3D, -0.057098388671875),
        (0x50, 0x7F, 0.0856475830078125),
        (0x51, 0x7F, 0.0856475830078125),
        (0x52, 0x7F, 0.0856475830078125),
        (0x53, 0x3A, -0.057098388671875),
        (0x53, 0x3B, -0.057098388671875),
        (0x53, 0x3D, -0.057098388671875),
        (0x53, 0x7F, 0.0856475830078125),
        (0x54, 0x3A, -0.057098388671875),
        (0x54, 0x3B, -0.057098388671875),
        (0x54, 0x3D, -0.0285491943359375),
        (0x54, 0x7F, 0.0856475830078125),
        (0x55, 0x3A, -0.11419677734375),
        (0x55, 0x3B, -0.11419677734375),
        (0x55, 0x3D, -0.057098388671875),
        (0x55, 0x7F, 0.0285491943359375),
        (0x56, 0x3A, -0.171295166015625),
        (0x56, 0x3B, -0.171295166015625),
        (0x56, 0x3D, -0.11419677734375),
        (0x57, 0x3A, -0.171295166015625),
        (0x57, 0x3B, -0.171295166015625),
        (0x57, 0x3D, -0.11419677734375),
        (0x58, 0x3A, -0.057098388671875),
        (0x58, 0x3B, -0.057098388671875),
        (0x58, 0x3D, -0.0856475830078125),
        (0x58, 0x7F, 0.0856475830078125),
        (0x59, 0x3A, -0.171295166015625),
        (0x59, 0x3B, -0.171295166015625),
        (0x59, 0x3D, -0.11419677734375),
        (0x5A, 0x3A, -0.057098388671875),
        (0x5A, 0x3B, -0.057098388671875),
        (0x5A, 0x3D, -0.057098388671875),
        (0x5A, 0x7F, 0.0856475830078125),
        (0x60, 0x7F, 0.11419677734375),
        (0x63, 0x7F, 0.057098388671875),
        (0x64, 0x59, 0.057098388671875),
        (0x64, 0x5A, -0.057098388671875),
        (0x64, 0x66, -0.171295166015625),
        (0x64, 0x6A, -0.11419677734375),
        (0x64, 0x7F, 0.171295166015625),
        (0x65, 0x7F, 0.057098388671875),
        (0x66, 0x3A, -0.057098388671875),
        (0x66, 0x3B, -0.057098388671875),
        (0x66, 0x7F, 0.171295166015625),
        (0x67, 0x7F, 0.0285491943359375),
        (0x68, 0x7F, -0.0285491943359375),
        (0x6A, 0x3A, -0.057098388671875),
        (0x6A, 0x3B, -0.057098388671875),
        (0x6C, 0x7F, 0.0856475830078125),
        (0x6F, 0x7F, 0.057098388671875),
        (0x70, 0x7F, 0.0856475830078125),
        (0x71, 0x7F, 0.0856475830078125),
        (0x72, 0x3A, -0.057098388671875),
        (0x72, 0x3B, -0.057098388671875),
        (0x72, 0x7F, 0.057098388671875),
        (0x73, 0x7F, 0.057098388671875),
        (0x74, 0x7F, 0.0856475830078125),
        (0x75, 0x7F, 0.0285491943359375),
        (0x76, 0x7F, 0.0285491943359375),
        (0x77, 0x7F, 0.0856475830078125),
        (0x78, 0x7F, 0.0285491943359375),
        (0x79, 0x7F, 0.057098388671875),
        (0x7A, 0x7F, 0.057098388671875),
        (0x7B, 0x7F, 0.0285491943359375),
        (0x7C, 0x7F, 0.0856475830078125),
        (0x7D, 0x7F, 0.11419677734375),
    ],
};

/// `cmmi12.tfm`.
#[rustfmt::skip]
pub static CMMI12: TfmFont = TfmFont {
    name: "cmmi12",
    space: 0.0,
    skew_char: Some(0x7F),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305562973022461, 0.979156494140625],
    chars: &[
        Some(TfmChar { width: 0.6066513061523438, height: 0.6833333969116211, depth: 0.0, italic: 0.13599395751953125 }), // 0x00
        Some(TfmChar { width: 0.8159637451171875, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.7483024597167969, height: 0.6833333969116211, depth: 0.0, italic: 0.02719879150390625 }), // 0x02
        Some(TfmChar { width: 0.6796236038208008, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.728668212890625, height: 0.6833333969116211, depth: 0.0, italic: 0.075897216796875 }), // 0x04
        Some(TfmChar { width: 0.811279296875, height: 0.6833333969116211, depth: 0.0, italic: 0.07725811004638672 }), // 0x05
        Some(TfmChar { width: 0.7657918930053711, height: 0.6833333969116211, depth: 0.0, italic: 0.058623313903808594 }), // 0x06
        Some(TfmChar { width: 0.5711746215820312, height: 0.6833333969116211, depth: 0.0, italic: 0.13599395751953125 }), // 0x07
        Some(TfmChar { width: 0.65277099609375, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.5980348587036133, height: 0.6833333969116211, depth: 0.0, italic: 0.10913372039794922 }), // 0x09
        Some(TfmChar { width: 0.7576322555541992, height: 0.6833333969116211, depth: 0.0, italic: 0.05046367645263672 }), // 0x0A
        Some(TfmChar { width: 0.6227941513061523, height: 0.4305562973022461, depth: 0.0, italic: 0.006366729736328125 }), // 0x0B
        Some(TfmChar { width: 0.5527725219726562, height: 0.6944437026977539, depth: 0.1944437026977539, italic: 0.05543994903564453 }), // 0x0C
        Some(TfmChar { width: 0.5078887939453125, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0543975830078125 }), // 0x0D
        Some(TfmChar { width: 0.43367481231689453, height: 0.6944437026977539, depth: 0.0, italic: 0.037499427795410156 }), // 0x0E
        Some(TfmChar { width: 0.3953666687011719, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.4276580810546875, height: 0.6944437026977539, depth: 0.1944437026977539, italic: 0.07552242279052734 }), // 0x10
        Some(TfmChar { width: 0.4830970764160156, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.035880088806152344 }), // 0x11
        Some(TfmChar { width: 0.4563026428222656, height: 0.6944437026977539, depth: 0.0, italic: 0.02719879150390625 }), // 0x12
        Some(TfmChar { width: 0.34606170654296875, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5636520385742188, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5711746215820312, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.5891151428222656, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.4837923049926758, height: 0.4305562973022461, depth: 0.0, italic: 0.0630788803100586 }), // 0x17
        Some(TfmChar { width: 0.4276580810546875, height: 0.6944437026977539, depth: 0.1944437026977539, italic: 0.048323631286621094 }), // 0x18
        Some(TfmChar { width: 0.5554351806640625, height: 0.4305562973022461, depth: 0.0, italic: 0.035880088806152344 }), // 0x19
        Some(TfmChar { width: 0.505030632019043, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.5565338134765625, height: 0.4305562973022461, depth: 0.0, italic: 0.035880088806152344 }), // 0x1B
        Some(TfmChar { width: 0.42522716522216797, height: 0.4305562973022461, depth: 0.0, italic: 0.11169052124023438 }), // 0x1C
        Some(TfmChar { width: 0.5277719497680664, height: 0.4305562973022461, depth: 0.0, italic: 0.035880088806152344 }), // 0x1D
        Some(TfmChar { width: 0.5795068740844727, height: 0.6944437026977539, depth: 0.1944437026977539, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.6134214401245117, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.6365671157836914, height: 0.6944437026977539, depth: 0.1944437026977539, italic: 0.035880088806152344 }), // 0x20
        Some(TfmChar { width: 0.609715461730957, height: 0.4305562973022461, depth: 0.0, italic: 0.035880088806152344 }), // 0x21
        Some(TfmChar { width: 0.45821380615234375, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.5770769119262695, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.8089056015014648, height: 0.4305562973022461, depth: 0.0, italic: 0.02719879150390625 }), // 0x24
        Some(TfmChar { width: 0.505030632019043, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.35416126251220703, height: 0.4305562973022461, depth: 0.09722232818603516, italic: 0.08044052124023438 }), // 0x26
        Some(TfmChar { width: 0.6414270401000977, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.979156494140625, height: 0.3609018325805664, depth: -0.1390981674194336, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.979156494140625, height: 0.3609018325805664, depth: -0.1390981674194336, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.979156494140625, height: 0.3609018325805664, depth: -0.1390981674194336, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.979156494140625, height: 0.3609018325805664, depth: -0.1390981674194336, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.2719879150390625, height: 0.45347118377685547, depth: -0.04652881622314453, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.2719879150390625, height: 0.45347118377685547, depth: -0.04652881622314453, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.4895782470703125, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.4895782470703125, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.2719879150390625, height: 0.09722232818603516, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.2719879150390625, height: 0.09722232818603516, depth: 0.1944437026977539, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.761566162109375, height: 0.526667594909668, depth: 0.02666759490966797, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.4895782470703125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.761566162109375, height: 0.526667594909668, depth: 0.02666759490966797, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.4895782470703125, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.5168924331665039, height: 0.6944437026977539, depth: 0.0, italic: 0.0543975830078125 }), // 0x40
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.7438592910766602, height: 0.6833333969116211, depth: 0.0, italic: 0.05046367645263672 }), // 0x42
        Some(TfmChar { width: 0.7005376815795898, height: 0.6833333969116211, depth: 0.0, italic: 0.07181644439697266 }), // 0x43
        Some(TfmChar { width: 0.8129796981811523, height: 0.6833333969116211, depth: 0.0, italic: 0.02719879150390625 }), // 0x44
        Some(TfmChar { width: 0.7248201370239258, height: 0.6833333969116211, depth: 0.0, italic: 0.058623313903808594 }), // 0x45
        Some(TfmChar { width: 0.63385009765625, height: 0.6833333969116211, depth: 0.0, italic: 0.13599395751953125 }), // 0x46
        Some(TfmChar { width: 0.7723541259765625, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.811279296875, height: 0.6833333969116211, depth: 0.0, italic: 0.07725811004638672 }), // 0x48
        Some(TfmChar { width: 0.4318552017211914, height: 0.6833333969116211, depth: 0.0, italic: 0.07861709594726562 }), // 0x49
        Some(TfmChar { width: 0.5412015914916992, height: 0.6833333969116211, depth: 0.0, italic: 0.09259319305419922 }), // 0x4A
        Some(TfmChar { width: 0.833038330078125, height: 0.6833333969116211, depth: 0.0, italic: 0.07181644439697266 }), // 0x4B
        Some(TfmChar { width: 0.6661977767944336, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.9472732543945312, height: 0.6833333969116211, depth: 0.0, italic: 0.10445690155029297 }), // 0x4D
        Some(TfmChar { width: 0.7840805053710938, height: 0.6833333969116211, depth: 0.0, italic: 0.10445690155029297 }), // 0x4E
        Some(TfmChar { width: 0.7483024597167969, height: 0.6833333969116211, depth: 0.0, italic: 0.02719879150390625 }), // 0x4F
        Some(TfmChar { width: 0.6311302185058594, height: 0.6833333969116211, depth: 0.0, italic: 0.13599395751953125 }), // 0x50
        Some(TfmChar { width: 0.7755012512207031, height: 0.6833333969116211, depth: 0.1944437026977539, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7452926635742188, height: 0.6833333969116211, depth: 0.0, italic: 0.00823211669921875 }), // 0x52
        Some(TfmChar { width: 0.6021947860717773, height: 0.6833333969116211, depth: 0.0, italic: 0.05821704864501953 }), // 0x53
        Some(TfmChar { width: 0.5738945007324219, height: 0.6833333969116211, depth: 0.0, italic: 0.13599395751953125 }), // 0x54
        Some(TfmChar { width: 0.6650056838989258, height: 0.6833333969116211, depth: 0.0, italic: 0.10445690155029297 }), // 0x55
        Some(TfmChar { width: 0.5708284378051758, height: 0.6833333969116211, depth: 0.0, italic: 0.21759033203125 }), // 0x56
        Some(TfmChar { width: 0.924412727355957, height: 0.6833333969116211, depth: 0.0, italic: 0.13599395751953125 }), // 0x57
        Some(TfmChar { width: 0.8126382827758789, height: 0.6833333969116211, depth: 0.0, italic: 0.07861709594726562 }), // 0x58
        Some(TfmChar { width: 0.5681085586547852, height: 0.6833333969116211, depth: 0.0, italic: 0.21759033203125 }), // 0x59
        Some(TfmChar { width: 0.670191764831543, height: 0.6833333969116211, depth: 0.0, italic: 0.07181644439697266 }), // 0x5A
        Some(TfmChar { width: 0.3807830810546875, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.3807830810546875, height: 0.6944437026977539, depth: 0.1944437026977539, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.3807830810546875, height: 0.6944437026977539, depth: 0.1944437026977539, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.979156494140625, height: 0.3576393127441406, depth: -0.14236068725585938, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.979156494140625, height: 0.3576393127441406, depth: -0.14236068725585938, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.4108772277832031, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.5139989852905273, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.41631412506103516, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.42140674591064453, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.5087919235229492, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.4538154602050781, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.482635498046875, height: 0.6944437026977539, depth: 0.1944437026977539, italic: 0.10676956176757812 }), // 0x66
        Some(TfmChar { width: 0.4688606262207031, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.035880088806152344 }), // 0x67
        Some(TfmChar { width: 0.5636520385742188, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.3340339660644531, height: 0.6555900573730469, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.40509033203125, height: 0.6555900573730469, depth: 0.1944437026977539, italic: 0.056258201599121094 }), // 0x6A
        Some(TfmChar { width: 0.5092544555664062, height: 0.6944437026977539, depth: 0.0, italic: 0.033565521240234375 }), // 0x6B
        Some(TfmChar { width: 0.29166412353515625, height: 0.6944437026977539, depth: 0.0, italic: 0.021991729736328125 }), // 0x6C
        Some(TfmChar { width: 0.8564720153808594, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.5844841003417969, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.47071170806884766, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.49143123626708984, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.4341392517089844, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.035880088806152344 }), // 0x71
        Some(TfmChar { width: 0.4412574768066406, height: 0.4305562973022461, depth: 0.0, italic: 0.02719879150390625 }), // 0x72
        Some(TfmChar { width: 0.4612236022949219, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.35358428955078125, height: 0.6150808334350586, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.5572853088378906, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.4733743667602539, height: 0.4305562973022461, depth: 0.0, italic: 0.035880088806152344 }), // 0x76
        Some(TfmChar { width: 0.6999349594116211, height: 0.4305562973022461, depth: 0.0, italic: 0.026909828186035156 }), // 0x77
        Some(TfmChar { width: 0.5564193725585938, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.47742557525634766, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.035880088806152344 }), // 0x79
        Some(TfmChar { width: 0.45485687255859375, height: 0.4305562973022461, depth: 0.0, italic: 0.04456043243408203 }), // 0x7A
        Some(TfmChar { width: 0.3124961853027344, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.37789154052734375, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.6233739852905273, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.4895782470703125, height: 0.7127771377563477, depth: 0.0, italic: 0.15381908416748047 }), // 0x7E
        Some(TfmChar { width: 0.2719879150390625, height: 0.6944437026977539, depth: 0.0, italic: 0.3927450180053711 }), // 0x7F
    ],
    kerns: &[
        (0x00, 0x3A, -0.108795166015625),
        (0x00, 0x3B, -0.108795166015625),
        (0x00, 0x3D, -0.0543975830078125),
        (0x00, 0x7F, 0.08159637451171875),
        (0x01, 0x7F, 0.1631927490234375),
        (0x02, 0x7F, 0.08159637451171875),
        (0x03, 0x7F, 0.1631927490234375),
        (0x04, 0x7F, 0.08159637451171875),
        (0x05, 0x3A, -0.0543975830078125),
        (0x05, 0x3B, -0.0543975830078125),
        (0x05, 0x3D, -0.0543975830078125),
        (0x05, 0x7F, 0.0543975830078125),
        (0x06, 0x7F, 0.08159637451171875),
        (0x07, 0x3A, -0.108795166015625),
        (0x07, 0x3B, -0.108795166015625),
        (0x07, 0x3D, -0.0543975830078125),
        (0x07, 0x7F, 0.0543975830078125),
        (0x08, 0x7F, 0.08159637451171875),
        (0x09, 0x3A, -0.0543975830078125),
        (0x09, 0x3B, -0.0543975830078125),
        (0x09, 0x3D, -0.0543975830078125),
        (0x09, 0x7F, 0.0543975830078125),
        (0x0A, 0x7F, 0.08159637451171875),
        (0x0B, 0x7F, 0.02719879150390625),
        (0x0C, 0x7F, 0.08159637451171875),
        (0x0E, 0x3A, -0.0543975830078125),
        (0x0E, 0x3B, -0.0543975830078125),
        (0x0E, 0x7F, 0.0543975830078125),
        (0x0F, 0x7F, 0.0543975830078125),
        (0x10, 0x7F, 0.08159637451171875),
        (0x11, 0x7F, 0.0543975830078125),
        (0x12, 0x7F, 0.08159637451171875),
        (0x13, 0x7F, 0.0543975830078125),
        (0x16, 0x7F, 0.02719879150390625),
        (0x17, 0x3A, -0.0543975830078125),
        (0x17, 0x3B, -0.0543975830078125),
        (0x17, 0x7F, 0.02719879150390625),
        (0x18, 0x7F, 0.108795166015625),
        (0x1A, 0x7F, 0.08159637451171875),
        (0x1B, 0x3A, -0.0543975830078125),
        (0x1B, 0x3B, -0.0543975830078125),
        (0x1C, 0x3A, -0.0543975830078125),
        (0x1C, 0x3B, -0.0543975830078125),
        (0x1C, 0x7F, 0.02719879150390625),
        (0x1D, 0x7F, 0.02719879150390625),
        (0x1E, 0x7F, 0.08159637451171875),
        (0x1F, 0x7F, 0.0543975830078125),
        (0x20, 0x7F, 0.108795166015625),
        (0x22, 0x7F, 0.08159637451171875),
        (0x23, 0x7F, 0.08159637451171875),
        (0x25, 0x7F, 0.08159637451171875),
        (0x26, 0x7F, 0.08159637451171875),
        (0x27, 0x7F, 0.08159637451171875),
        (0x3D, 0x01, -0.0543975830078125),
        (0x3D, 0x41, -0.0543975830078125),
        (0x3D, 0x4D, -0.0543975830078125),
        (0x3D, 0x4E, -0.0543975830078125),
        (0x3D, 0x59, 0.0543975830078125),
        (0x3D, 0x5A, -0.0543975830078125),
        (0x40, 0x7F, 0.08159637451171875),
        (0x41, 0x7F, 0.13599395751953125),
        (0x42, 0x7F, 0.08159637451171875),
        (0x43, 0x3A, -0.0543975830078125),
        (0x43, 0x3B, -0.0543975830078125),
        (0x43, 0x3D, -0.02719879150390625),
        (0x43, 0x7F, 0.08159637451171875),
        (0x44, 0x7F, 0.0543975830078125),
        (0x45, 0x7F, 0.08159637451171875),
        (0x46, 0x3A, -0.108795166015625),
        (0x46, 0x3B, -0.108795166015625),
        (0x46, 0x3D, -0.0543975830078125),
        (0x46, 0x7F, 0.08159637451171875),
        (0x47, 0x7F, 0.08159637451171875),
        (0x48, 0x3A, -0.0543975830078125),
        (0x48, 0x3B, -0.0543975830078125),
        (0x48, 0x3D, -0.0543975830078125),
        (0x48, 0x7F, 0.0543975830078125),
        (0x49, 0x7F, 0.108795166015625),
        (0x4A, 0x3A, -0.108795166015625),
        (0x4A, 0x3B, -0.108795166015625),
        (0x4A, 0x3D, -0.0543975830078125),
        (0x4A, 0x7F, 0.1631927490234375),
        (0x4B, 0x3A, -0.0543975830078125),
        (0x4B, 0x3B, -0.0543975830078125),
        (0x4B, 0x3D, -0.0543975830078125),
        (0x4B, 0x7F, 0.0543975830078125),
        (0x4C, 0x7F, 0.02719879150390625),
        (0x4D, 0x3A, -0.0543975830078125),
        (0x4D, 0x3B, -0.0543975830078125),
        (0x4D, 0x3D, -0.0543975830078125),
        (0x4D, 0x7F, 0.08159637451171875),
        (0x4E, 0x3A, -0.0543975830078125),
        (0x4E, 0x3B, -0.0543975830078125),
        (0x4E, 0x3D, -0.08159637451171875),
        (0x4E, 0x7F, 0.08159637451171875),
        (0x4F, 0x7F, 0.08159637451171875),
        (0x50, 0x3A, -0.108795166015625),
        (0x50, 0x3B, -0.108795166015625),
        (0x50, 0x3D, -0.0543975830078125),
        (0x50, 0x7F, 0.08159637451171875),
        (0x51, 0x7F, 0.08159637451171875),
        (0x52, 0x7F, 0.08159637451171875),
        (0x53, 0x3A, -0.0543975830078125),
        (0x53, 0x3B, -0.0543975830078125),
        (0x53, 0x3D, -0.0543975830078125),
        (0x53, 0x7F, 0.08159637451171875),
        (0x54, 0x3A, -0.0543975830078125),
        (0x54, 0x3B, -0.0543975830078125),
        (0x54, 0x3D, -0.02719879150390625),
        (0x54, 0x7F, 0.08159637451171875),
        (0x55, 0x3A, -0.108795166015625),
        (0x55, 0x3B, -0.108795166015625),
        (0x55, 0x3D, -0.0543975830078125),
        (0x55, 0x7F, 0.02719879150390625),
        (0x56, 0x3A, -0.1631927490234375),
        (0x56, 0x3B, -0.1631927490234375),
        (0x56, 0x3D, -0.108795166015625),
        (0x57, 0x3A, -0.1631927490234375),
        (0x57, 0x3B, -0.1631927490234375),
        (0x57, 0x3D, -0.108795166015625),
        (0x58, 0x3A, -0.0543975830078125),
        (0x58, 0x3B, -0.0543975830078125),
        (0x58, 0x3D, -0.08159637451171875),
        (0x58, 0x7F, 0.08159637451171875),
        (0x59, 0x3A, -0.1631927490234375),
        (0x59, 0x3B, -0.1631927490234375),
        (0x59, 0x3D, -0.108795166015625),
        (0x5A, 0x3A, -0.0543975830078125),
        (0x5A, 0x3B, -0.0543975830078125),
        (0x5A, 0x3D, -0.0543975830078125),
        (0x5A, 0x7F, 0.08159637451171875),
        (0x60, 0x7F, 0.108795166015625),
        (0x63, 0x7F, 0.0543975830078125),
        (0x64, 0x59, 0.0543975830078125),
        (0x64, 0x5A, -0.0543975830078125),
        (0x64, 0x66, -0.1631927490234375),
        (0x64, 0x6A, -0.108795166015625),
        (0x64, 0x7F, 0.1631927490234375),
        (0x65, 0x7F, 0.0543975830078125),
        (0x66, 0x3A, -0.0543975830078125),
        (0x66, 0x3B, -0.0543975830078125),
        (0x66, 0x7F, 0.1631927490234375),
        (0x67, 0x7F, 0.02719879150390625),
        (0x68, 0x7F, -0.02719879150390625),
        (0x6A, 0x3A, -0.0543975830078125),
        (0x6A, 0x3B, -0.0543975830078125),
        (0x6C, 0x7F, 0.08159637451171875),
        (0x6F, 0x7F, 0.0543975830078125),
        (0x70, 0x7F, 0.08159637451171875),
        (0x71, 0x7F, 0.08159637451171875),
        (0x72, 0x3A, -0.0543975830078125),
        (0x72, 0x3B, -0.0543975830078125),
        (0x72, 0x7F, 0.0543975830078125),
        (0x73, 0x7F, 0.0543975830078125),
        (0x74, 0x7F, 0.08159637451171875),
        (0x75, 0x7F, 0.02719879150390625),
        (0x76, 0x7F, 0.02719879150390625),
        (0x77, 0x7F, 0.08159637451171875),
        (0x78, 0x7F, 0.02719879150390625),
        (0x79, 0x7F, 0.0543975830078125),
        (0x7A, 0x7F, 0.0543975830078125),
        (0x7B, 0x7F, 0.02719879150390625),
        (0x7C, 0x7F, 0.08159637451171875),
        (0x7D, 0x7F, 0.108795166015625),
    ],
};

/// `cmsy10.tfm`.
#[rustfmt::skip]
pub static CMSY10: TfmFont = TfmFont {
    name: "cmsy10",
    space: 0.0,
    skew_char: Some(0x30),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305553436279297, 1.0000028610229492, 0.0, 0.6765079498291016, 0.39373207092285156, 0.44373130798339844, 0.6859512329101562, 0.34484100341796875, 0.41289234161376953, 0.36289215087890625, 0.28888893127441406, 0.14999961853027344, 0.24721717834472656, 0.3861083984375, 0.05000019073486328, 2.3899993896484375, 1.010000228881836, 0.25],
    chars: &[
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.27777862548828125, height: 0.4444456100463867, depth: -0.05555438995361328, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.5000019073486328, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.5000019073486328, height: 0.4444456100463867, depth: -0.05555438995361328, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 1.0000028610229492, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.5000019073486328, height: 0.4444456100463867, depth: -0.05555438995361328, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.5000019073486328, height: 0.4444456100463867, depth: -0.05555438995361328, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.7777805328369141, height: 0.4637479782104492, depth: -0.03625202178955078, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.7777805328369141, height: 0.4637479782104492, depth: -0.03625202178955078, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.7777805328369141, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.7777805328369141, height: 0.4831218719482422, depth: -0.016878128051757812, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.7777805328369141, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.7777805328369141, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.0000028610229492, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.0000028610229492, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.7777805328369141, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.7777805328369141, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.0000028610229492, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.0000028610229492, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.7777805328369141, height: 0.4637479782104492, depth: -0.03625202178955078, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.6111125946044922, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.6111125946044922, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.0000028610229492, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 1.0000028610229492, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.7777805328369141, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.27499961853027344, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.0000028610229492, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.6666688919067383, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.6666688919067383, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.8888912200927734, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.8888912200927734, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.0, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.0, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5555572509765625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5555572509765625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.6666688919067383, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.722224235534668, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.722224235534668, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.7777805328369141, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.7777805328369141, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.6111125946044922, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.7984714508056641, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.6568098068237305, height: 0.6833324432373047, depth: 0.0, italic: 0.030412673950195312 }), // 0x42
        Some(TfmChar { width: 0.5265293121337891, height: 0.6833324432373047, depth: 0.0, italic: 0.05833625793457031 }), // 0x43
        Some(TfmChar { width: 0.771392822265625, height: 0.6833324432373047, depth: 0.0, italic: 0.02777862548828125 }), // 0x44
        Some(TfmChar { width: 0.5277814865112305, height: 0.6833324432373047, depth: 0.0, italic: 0.08944129943847656 }), // 0x45
        Some(TfmChar { width: 0.7187480926513672, height: 0.6833324432373047, depth: 0.0, italic: 0.09930610656738281 }), // 0x46
        Some(TfmChar { width: 0.5948657989501953, height: 0.6833324432373047, depth: 0.09722328186035156, italic: 0.05930328369140625 }), // 0x47
        Some(TfmChar { width: 0.844517707824707, height: 0.6833324432373047, depth: 0.0, italic: 0.009654045104980469 }), // 0x48
        Some(TfmChar { width: 0.5445156097412109, height: 0.6833324432373047, depth: 0.0, italic: 0.073822021484375 }), // 0x49
        Some(TfmChar { width: 0.6777801513671875, height: 0.6833324432373047, depth: 0.09722328186035156, italic: 0.1847209930419922 }), // 0x4A
        Some(TfmChar { width: 0.7619504928588867, height: 0.6833324432373047, depth: 0.0, italic: 0.014445304870605469 }), // 0x4B
        Some(TfmChar { width: 0.6897249221801758, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.2009000778198242, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.820490837097168, height: 0.6833324432373047, depth: 0.0, italic: 0.14735984802246094 }), // 0x4E
        Some(TfmChar { width: 0.7961139678955078, height: 0.6833324432373047, depth: 0.0, italic: 0.02777862548828125 }), // 0x4F
        Some(TfmChar { width: 0.6955595016479492, height: 0.6833324432373047, depth: 0.0, italic: 0.08222198486328125 }), // 0x50
        Some(TfmChar { width: 0.8166704177856445, height: 0.6833324432373047, depth: 0.09722328186035156, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.847503662109375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.6055583953857422, height: 0.6833324432373047, depth: 0.0, italic: 0.07499504089355469 }), // 0x53
        Some(TfmChar { width: 0.5446443557739258, height: 0.6833324432373047, depth: 0.0, italic: 0.2541675567626953 }), // 0x54
        Some(TfmChar { width: 0.6258316040039062, height: 0.6833324432373047, depth: 0.0, italic: 0.09930610656738281 }), // 0x55
        Some(TfmChar { width: 0.6127824783325195, height: 0.6833324432373047, depth: 0.0, italic: 0.08222198486328125 }), // 0x56
        Some(TfmChar { width: 0.9877834320068359, height: 0.6833324432373047, depth: 0.0, italic: 0.08222198486328125 }), // 0x57
        Some(TfmChar { width: 0.7132968902587891, height: 0.6833324432373047, depth: 0.0, italic: 0.14642810821533203 }), // 0x58
        Some(TfmChar { width: 0.6683368682861328, height: 0.6833324432373047, depth: 0.09722328186035156, italic: 0.08222198486328125 }), // 0x59
        Some(TfmChar { width: 0.7247257232666016, height: 0.6833324432373047, depth: 0.0, italic: 0.07944297790527344 }), // 0x5A
        Some(TfmChar { width: 0.6666688919067383, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.6666688919067383, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.6666688919067383, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.6666688919067383, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.6666688919067383, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.6111125946044922, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.6111125946044922, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.4444456100463867, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.4444456100463867, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.4444456100463867, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.4444456100463867, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.38889026641845703, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.38889026641845703, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.27777862548828125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.6111125946044922, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.27777862548828125, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.8333358764648438, height: 0.03999900817871094, depth: 0.9600009918212891, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.8333358764648438, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.4166688919067383, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.11111164093017578 }), // 0x73
        Some(TfmChar { width: 0.6666688919067383, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.6666688919067383, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.44444751739501953, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.4444456100463867, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.4444456100463867, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.6111125946044922, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.7777805328369141, height: 0.6944446563720703, depth: 0.12962913513183594, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.7777805328369141, height: 0.6944446563720703, depth: 0.12962913513183594, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.7777805328369141, height: 0.6944446563720703, depth: 0.12962913513183594, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.7777805328369141, height: 0.6944446563720703, depth: 0.12962913513183594, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x41, 0x30, 0.19445037841796875),
        (0x42, 0x30, 0.13889312744140625),
        (0x43, 0x30, 0.13889312744140625),
        (0x44, 0x30, 0.08333587646484375),
        (0x45, 0x30, 0.111114501953125),
        (0x46, 0x30, 0.111114501953125),
        (0x47, 0x30, 0.111114501953125),
        (0x48, 0x30, 0.111114501953125),
        (0x49, 0x30, 0.02777862548828125),
        (0x4A, 0x30, 0.1666717529296875),
        (0x4B, 0x30, 0.0555572509765625),
        (0x4C, 0x30, 0.13889312744140625),
        (0x4D, 0x30, 0.13889312744140625),
        (0x4E, 0x30, 0.08333587646484375),
        (0x4F, 0x30, 0.111114501953125),
        (0x50, 0x30, 0.08333587646484375),
        (0x51, 0x30, 0.111114501953125),
        (0x52, 0x30, 0.08333587646484375),
        (0x53, 0x30, 0.13889312744140625),
        (0x54, 0x30, 0.02777862548828125),
        (0x55, 0x30, 0.08333587646484375),
        (0x56, 0x30, 0.02777862548828125),
        (0x57, 0x30, 0.08333587646484375),
        (0x58, 0x30, 0.13889312744140625),
        (0x59, 0x30, 0.08333587646484375),
        (0x5A, 0x30, 0.13889312744140625),
    ],
};

/// `cmsy7.tfm`.
#[rustfmt::skip]
pub static CMSY7: TfmFont = TfmFont {
    name: "cmsy7",
    space: 0.0,
    skew_char: Some(0x30),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305553436279297, 1.1706409454345703, 0.0, 0.7323131561279297, 0.38390254974365234, 0.4712047576904297, 0.7521495819091797, 0.34421443939208984, 0.5025434494018555, 0.43111515045166016, 0.28571414947509766, 0.14285755157470703, 0.28571414947509766, 0.35317134857177734, 0.07142829895019531, 1.6999988555908203, 1.1571435928344727, 0.25],
    chars: &[
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.33928775787353516, height: 0.46527957916259766, depth: -0.034720420837402344, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.5853204727172852, height: 0.46527767181396484, depth: -0.034722328186035156, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.5853204727172852, height: 0.46527957916259766, depth: -0.034720420837402344, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.8928613662719727, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 1.1388940811157227, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.5853204727172852, height: 0.46527957916259766, depth: -0.034720420837402344, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.5853204727172852, height: 0.46527957916259766, depth: -0.034720420837402344, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.8928613662719727, height: 0.49387073516845703, depth: -0.006129264831542969, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.8928613662719727, height: 0.49387073516845703, depth: -0.006129264831542969, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.8928613662719727, height: 0.6854686737060547, depth: 0.1854686737060547, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.8928613662719727, height: 0.6854686737060547, depth: 0.1854686737060547, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.8928613662719727, height: 0.6854686737060547, depth: 0.1854686737060547, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.8928613662719727, height: 0.6854686737060547, depth: 0.1854686737060547, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.8928613662719727, height: 0.6854686737060547, depth: 0.1854686737060547, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.8928613662719727, height: 0.6854686737060547, depth: 0.1854686737060547, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.8928613662719727, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.8928613662719727, height: 0.5158281326293945, depth: 0.01582813262939453, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.8928613662719727, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.8928613662719727, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.1388940811157227, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.1388940811157227, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.8928613662719727, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.8928613662719727, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.5853204727172852, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.5853204727172852, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.1388940811157227, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.1388940811157227, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.8928613662719727, height: 0.49387073516845703, depth: -0.006129264831542969, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.7083368301391602, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.7083368301391602, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.1388940811157227, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 1.1388940811157227, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.8928613662719727, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.32936573028564453, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.1388940811157227, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.7698450088500977, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.7698450088500977, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 1.0158777236938477, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 1.0158777236938477, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.0, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.0, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.6468286514282227, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.6468286514282227, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.7698450088500977, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.5853204727172852, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.8313531875610352, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.8313531875610352, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.8928613662719727, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.8928613662719727, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.7083368301391602, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.9176025390625, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.7534379959106445, height: 0.6833343505859375, depth: 0.0, italic: 0.0245208740234375 }), // 0x42
        Some(TfmChar { width: 0.6201848983764648, height: 0.6833343505859375, depth: 0.0, italic: 0.04994392395019531 }), // 0x43
        Some(TfmChar { width: 0.8894500732421875, height: 0.6833343505859375, depth: 0.0, italic: 0.030755043029785156 }), // 0x44
        Some(TfmChar { width: 0.6160783767700195, height: 0.6833343505859375, depth: 0.0, italic: 0.084381103515625 }), // 0x45
        Some(TfmChar { width: 0.818354606628418, height: 0.6833343505859375, depth: 0.0, italic: 0.10079288482666016 }), // 0x46
        Some(TfmChar { width: 0.6885175704956055, height: 0.6833343505859375, depth: 0.09722232818603516, italic: 0.05650520324707031 }), // 0x47
        Some(TfmChar { width: 0.9786443710327148, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.6465015411376953, height: 0.6833343505859375, depth: 0.0, italic: 0.06493282318115234 }), // 0x49
        Some(TfmChar { width: 0.7821483612060547, height: 0.6833343505859375, depth: 0.09722232818603516, italic: 0.18620967864990234 }), // 0x4A
        Some(TfmChar { width: 0.871678352355957, height: 0.6833343505859375, depth: 0.0, italic: 0.0013513565063476562 }), // 0x4B
        Some(TfmChar { width: 0.791712760925293, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.3426885604858398, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.9355707168579102, height: 0.6833343505859375, depth: 0.0, italic: 0.15033817291259766 }), // 0x4E
        Some(TfmChar { width: 0.9058380126953125, height: 0.6833343505859375, depth: 0.0, italic: 0.030755043029785156 }), // 0x4F
        Some(TfmChar { width: 0.8091516494750977, height: 0.6833343505859375, depth: 0.0, italic: 0.08371162414550781 }), // 0x50
        Some(TfmChar { width: 0.9359197616577148, height: 0.6833343505859375, depth: 0.09722232818603516, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.9810352325439453, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.7021875381469727, height: 0.6833343505859375, depth: 0.0, italic: 0.072052001953125 }), // 0x53
        Some(TfmChar { width: 0.6478204727172852, height: 0.6833343505859375, depth: 0.0, italic: 0.26309680938720703 }), // 0x54
        Some(TfmChar { width: 0.717799186706543, height: 0.6833343505859375, depth: 0.0, italic: 0.10079288482666016 }), // 0x55
        Some(TfmChar { width: 0.7199268341064453, height: 0.6833343505859375, depth: 0.0, italic: 0.08371162414550781 }), // 0x56
        Some(TfmChar { width: 1.1351079940795898, height: 0.6833343505859375, depth: 0.0, italic: 0.08371162414550781 }), // 0x57
        Some(TfmChar { width: 0.8188562393188477, height: 0.6833343505859375, depth: 0.0, italic: 0.14642810821533203 }), // 0x58
        Some(TfmChar { width: 0.7643718719482422, height: 0.6833343505859375, depth: 0.09722232818603516, italic: 0.08371162414550781 }), // 0x59
        Some(TfmChar { width: 0.8231420516967773, height: 0.6833343505859375, depth: 0.0, italic: 0.08063411712646484 }), // 0x5A
        Some(TfmChar { width: 0.7698450088500977, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.7698450088500977, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.7698450088500977, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.7698450088500977, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.7698450088500977, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.7083368301391602, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.7083368301391602, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.5238122940063477, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.5238122940063477, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.5238122940063477, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.5238122940063477, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.5853204727172852, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.5853204727172852, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.46230411529541016, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.46230411529541016, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.33928775787353516, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5853204727172852, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.5853204727172852, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.7083368301391602, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.5853204727172852, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.33928775787353516, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.9384965896606445, height: 0.04857063293457031, depth: 0.9514293670654297, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.8591327667236328, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.9543695449829102, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.49355602264404297, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.123016357421875 }), // 0x73
        Some(TfmChar { width: 0.7698450088500977, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.7698450088500977, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.8928613662719727, height: 0.6854686737060547, depth: 0.1854686737060547, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.8928613662719727, height: 0.6854686737060547, depth: 0.1854686737060547, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.5238142013549805, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5238122940063477, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.5238122940063477, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.7083368301391602, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.8928613662719727, height: 0.6944446563720703, depth: 0.12963008880615234, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.8928613662719727, height: 0.6944446563720703, depth: 0.12963008880615234, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.8928613662719727, height: 0.6944446563720703, depth: 0.12963008880615234, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.8928613662719727, height: 0.6944446563720703, depth: 0.12963008880615234, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x41, 0x30, 0.2152862548828125),
        (0x42, 0x30, 0.1537761688232422),
        (0x43, 0x30, 0.1537761688232422),
        (0x44, 0x30, 0.09226512908935547),
        (0x45, 0x30, 0.12302112579345703),
        (0x46, 0x30, 0.12302112579345703),
        (0x47, 0x30, 0.12302112579345703),
        (0x48, 0x30, 0.12302112579345703),
        (0x49, 0x30, 0.030755043029785156),
        (0x4A, 0x30, 0.18453121185302734),
        (0x4B, 0x30, 0.06151008605957031),
        (0x4C, 0x30, 0.1537761688232422),
        (0x4D, 0x30, 0.1537761688232422),
        (0x4E, 0x30, 0.09226512908935547),
        (0x4F, 0x30, 0.12302112579345703),
        (0x50, 0x30, 0.09226512908935547),
        (0x51, 0x30, 0.12302112579345703),
        (0x52, 0x30, 0.09226512908935547),
        (0x53, 0x30, 0.1537761688232422),
        (0x54, 0x30, 0.030755043029785156),
        (0x55, 0x30, 0.09226512908935547),
        (0x56, 0x30, 0.030755043029785156),
        (0x57, 0x30, 0.09226512908935547),
        (0x58, 0x30, 0.1537761688232422),
        (0x59, 0x30, 0.09226512908935547),
        (0x5A, 0x30, 0.1537761688232422),
    ],
};

/// `cmsy5.tfm`.
#[rustfmt::skip]
pub static CMSY5: TfmFont = TfmFont {
    name: "cmsy5",
    space: 0.0,
    skew_char: Some(0x30),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305543899536133, 1.4722414016723633, 0.0, 0.9252414703369141, 0.3874635696411133, 0.5041322708129883, 1.0252408981323242, 0.5319089889526367, 0.5035552978515625, 0.40355491638183594, 0.2944459915161133, 0.19999980926513672, 0.39999961853027344, 0.49444580078125, 0.10000038146972656, 1.9799985885620117, 1.4200010299682617, 0.25],
    chars: &[
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.45833778381347656, height: 0.4930601119995117, depth: -0.006939888000488281, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.7361202239990234, height: 0.46527671813964844, depth: -0.03472328186035156, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.7361202239990234, height: 0.4930601119995117, depth: -0.006939888000488281, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 1.0833492279052734, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 1.3611326217651367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.7361202239990234, height: 0.4930601119995117, depth: -0.006939888000488281, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.7361202239990234, height: 0.4930601119995117, depth: -0.006939888000488281, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 1.0833492279052734, height: 0.5148067474365234, depth: 0.014806747436523438, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 1.0833492279052734, height: 0.5148067474365234, depth: 0.014806747436523438, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 1.0833492279052734, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 1.0833492279052734, height: 0.5384883880615234, depth: 0.03848838806152344, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.3611326217651367, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.3611326217651367, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.7361202239990234, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.7361202239990234, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.3611326217651367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.3611326217651367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 1.0833492279052734, height: 0.5148067474365234, depth: 0.014806747436523438, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.8750123977661133, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.8750123977661133, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.3611326217651367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 1.3611326217651367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 1.0833492279052734, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.4409761428833008, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.3611326217651367, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.9444580078125, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.9444580078125, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 1.2222414016723633, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 1.2222414016723633, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.0, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.0, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.8055667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.8055667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.9444580078125, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.7361202239990234, height: 0.75, depth: 0.05555438995361328, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 1.0139036178588867, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 1.0139036178588867, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 1.0833492279052734, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 1.0833492279052734, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.8750123977661133, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 1.113485336303711, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.9193172454833984, height: 0.6833343505859375, depth: 0.0, italic: 0.01665973663330078 }), // 0x42
        Some(TfmChar { width: 0.7820920944213867, height: 0.6833343505859375, depth: 0.0, italic: 0.03875160217285156 }), // 0x43
        Some(TfmChar { width: 1.0839080810546875, height: 0.6833343505859375, depth: 0.0, italic: 0.03472328186035156 }), // 0x44
        Some(TfmChar { width: 0.770843505859375, height: 0.6833343505859375, depth: 0.0, italic: 0.07763385772705078 }), // 0x45
        Some(TfmChar { width: 0.9882049560546875, height: 0.6833343505859375, depth: 0.0, italic: 0.10277748107910156 }), // 0x46
        Some(TfmChar { width: 0.8504304885864258, height: 0.6833343505859375, depth: 0.09722328186035156, italic: 0.05277442932128906 }), // 0x47
        Some(TfmChar { width: 1.1945343017578125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.8195285797119141, height: 0.6833343505859375, depth: 0.0, italic: 0.052154541015625 }), // 0x49
        Some(TfmChar { width: 0.9583463668823242, height: 0.6833343505859375, depth: 0.09722328186035156, italic: 0.18819618225097656 }), // 0x4A
        Some(TfmChar { width: 1.0550203323364258, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.9647369384765625, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.567861557006836, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 1.126058578491211, height: 0.6833343505859375, depth: 0.0, italic: 0.15430641174316406 }), // 0x4E
        Some(TfmChar { width: 1.089181900024414, height: 0.6833343505859375, depth: 0.0, italic: 0.03472328186035156 }), // 0x4F
        Some(TfmChar { width: 0.9976558685302734, height: 0.6833343505859375, depth: 0.0, italic: 0.085693359375 }), // 0x50
        Some(TfmChar { width: 1.1319608688354492, height: 0.6833343505859375, depth: 0.09722328186035156, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 1.1961336135864258, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.8680667877197266, height: 0.6833343505859375, depth: 0.0, italic: 0.06812095642089844 }), // 0x53
        Some(TfmChar { width: 0.8224334716796875, height: 0.6833343505859375, depth: 0.0, italic: 0.27500343322753906 }), // 0x54
        Some(TfmChar { width: 0.8737583160400391, height: 0.6833343505859375, depth: 0.0, italic: 0.10277748107910156 }), // 0x55
        Some(TfmChar { width: 0.8989048004150391, height: 0.6833343505859375, depth: 0.0, italic: 0.085693359375 }), // 0x56
        Some(TfmChar { width: 1.3676633834838867, height: 0.6833343505859375, depth: 0.0, italic: 0.085693359375 }), // 0x57
        Some(TfmChar { width: 0.9966402053833008, height: 0.6833343505859375, depth: 0.0, italic: 0.14642906188964844 }), // 0x58
        Some(TfmChar { width: 0.9294586181640625, height: 0.6833343505859375, depth: 0.09722328186035156, italic: 0.085693359375 }), // 0x59
        Some(TfmChar { width: 0.9914035797119141, height: 0.6833343505859375, depth: 0.0, italic: 0.08222007751464844 }), // 0x5A
        Some(TfmChar { width: 0.9444580078125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.9444580078125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.9444580078125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.9444580078125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.9444580078125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.8750123977661133, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.8750123977661133, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.6666746139526367, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.6666746139526367, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.6666746139526367, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.6666746139526367, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.7361202239990234, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.7361202239990234, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.59722900390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.59722900390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.45833778381347656, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.7361202239990234, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.7361202239990234, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.8750123977661133, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.7361202239990234, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.45833778381347656, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 1.0972414016723633, height: 0.055999755859375, depth: 0.944000244140625, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 1.0361270904541016, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 1.1527957916259766, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.6319522857666016, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.13889122009277344 }), // 0x73
        Some(TfmChar { width: 0.9444580078125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.9444580078125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.6666746139526367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.6666746139526367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.6666746139526367, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.8750123977661133, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 1.0833492279052734, height: 0.6944456100463867, depth: 0.12962913513183594, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 1.0833492279052734, height: 0.6944456100463867, depth: 0.12962913513183594, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 1.0833492279052734, height: 0.6944456100463867, depth: 0.12962913513183594, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 1.0833492279052734, height: 0.6944456100463867, depth: 0.12962913513183594, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x41, 0x30, 0.24306011199951172),
        (0x42, 0x30, 0.173614501953125),
        (0x43, 0x30, 0.173614501953125),
        (0x44, 0x30, 0.10416889190673828),
        (0x45, 0x30, 0.13889122009277344),
        (0x46, 0x30, 0.13889122009277344),
        (0x47, 0x30, 0.13889122009277344),
        (0x48, 0x30, 0.13889122009277344),
        (0x49, 0x30, 0.03472328186035156),
        (0x4A, 0x30, 0.20833778381347656),
        (0x4B, 0x30, 0.06944561004638672),
        (0x4C, 0x30, 0.173614501953125),
        (0x4D, 0x30, 0.173614501953125),
        (0x4E, 0x30, 0.10416889190673828),
        (0x4F, 0x30, 0.13889122009277344),
        (0x50, 0x30, 0.10416889190673828),
        (0x51, 0x30, 0.13889122009277344),
        (0x52, 0x30, 0.10416889190673828),
        (0x53, 0x30, 0.173614501953125),
        (0x54, 0x30, 0.03472328186035156),
        (0x55, 0x30, 0.10416889190673828),
        (0x56, 0x30, 0.03472328186035156),
        (0x57, 0x30, 0.10416889190673828),
        (0x58, 0x30, 0.173614501953125),
        (0x59, 0x30, 0.10416889190673828),
        (0x5A, 0x30, 0.173614501953125),
    ],
};

/// `cmsy6.tfm`.
#[rustfmt::skip]
pub static CMSY6: TfmFont = TfmFont {
    name: "cmsy6",
    space: 0.0,
    skew_char: Some(0x30),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305543899536133, 1.27777099609375, 0.0, 0.8126983642578125, 0.3923311233520508, 0.48492431640625, 0.863621711730957, 0.4293642044067383, 0.5029678344726562, 0.4196348190307617, 0.28703784942626953, 0.16666698455810547, 0.33333301544189453, 0.4120330810546875, 0.08333301544189453, 1.983332633972168, 1.350001335144043, 0.25],
    chars: &[
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.37962818145751953, height: 0.4768495559692383, depth: -0.02315044403076172, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.638885498046875, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.638885498046875, height: 0.4768495559692383, depth: -0.02315044403076172, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.962956428527832, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 1.2222137451171875, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.638885498046875, height: 0.4768495559692383, depth: -0.02315044403076172, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.638885498046875, height: 0.4768495559692383, depth: -0.02315044403076172, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.962956428527832, height: 0.5040283203125, depth: 0.0040283203125, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.962956428527832, height: 0.5040283203125, depth: 0.0040283203125, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.962956428527832, height: 0.7020845413208008, depth: 0.20208454132080078, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.962956428527832, height: 0.7020845413208008, depth: 0.20208454132080078, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.962956428527832, height: 0.7020845413208008, depth: 0.20208454132080078, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.962956428527832, height: 0.7020845413208008, depth: 0.20208454132080078, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.962956428527832, height: 0.7020845413208008, depth: 0.20208454132080078, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.962956428527832, height: 0.7020845413208008, depth: 0.20208454132080078, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.962956428527832, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.962956428527832, height: 0.5268478393554688, depth: 0.02684783935546875, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.962956428527832, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.962956428527832, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.2222137451171875, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.2222137451171875, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.962956428527832, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.962956428527832, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.638885498046875, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.638885498046875, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.2222137451171875, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.2222137451171875, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.962956428527832, height: 0.5040283203125, depth: 0.0040283203125, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.7685136795043945, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.7685136795043945, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.2222137451171875, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.2222137451171875, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 1.2222137451171875, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.962956428527832, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.3657407760620117, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.2222137451171875, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.8333282470703125, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.8333282470703125, height: 0.587986946105957, depth: 0.08798694610595703, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 1.092585563659668, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 1.092585563659668, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.0, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.0, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.703700065612793, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.703700065612793, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.8333282470703125, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.638885498046875, height: 0.75, depth: 0.05555438995361328, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.8981428146362305, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.8981428146362305, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.962956428527832, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.962956428527832, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.7685136795043945, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.9899415969848633, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.8132829666137695, height: 0.6833343505859375, depth: 0.0, italic: 0.02124500274658203 }), // 0x42
        Some(TfmChar { width: 0.678375244140625, height: 0.6833343505859375, depth: 0.0, italic: 0.04528331756591797 }), // 0x43
        Some(TfmChar { width: 0.9611968994140625, height: 0.6833343505859375, depth: 0.0, italic: 0.03240680694580078 }), // 0x44
        Some(TfmChar { width: 0.6712923049926758, height: 0.6833343505859375, depth: 0.0, italic: 0.081573486328125 }), // 0x45
        Some(TfmChar { width: 0.879852294921875, height: 0.6833343505859375, depth: 0.0, italic: 0.10162067413330078 }), // 0x46
        Some(TfmChar { width: 0.7467069625854492, height: 0.6833343505859375, depth: 0.09722423553466797, italic: 0.05495452880859375 }), // 0x47
        Some(TfmChar { width: 1.0593185424804688, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.7093229293823242, height: 0.6833343505859375, depth: 0.0, italic: 0.05961132049560547 }), // 0x49
        Some(TfmChar { width: 0.8462905883789062, height: 0.6833343505859375, depth: 0.09722423553466797, italic: 0.18703937530517578 }), // 0x4A
        Some(TfmChar { width: 0.9387941360473633, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.8545331954956055, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.4272308349609375, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 1.0056657791137695, height: 0.6833343505859375, depth: 0.0, italic: 0.15198993682861328 }), // 0x4E
        Some(TfmChar { width: 0.972956657409668, height: 0.6833343505859375, depth: 0.0, italic: 0.03240680694580078 }), // 0x4F
        Some(TfmChar { width: 0.8784208297729492, height: 0.6833343505859375, depth: 0.0, italic: 0.08453655242919922 }), // 0x50
        Some(TfmChar { width: 1.008326530456543, height: 0.6833343505859375, depth: 0.09722423553466797, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 1.0613813400268555, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.7620315551757812, height: 0.6833343505859375, depth: 0.0, italic: 0.07041454315185547 }), // 0x53
        Some(TfmChar { width: 0.7113037109375, height: 0.6833343505859375, depth: 0.0, italic: 0.26805591583251953 }), // 0x54
        Some(TfmChar { width: 0.7744340896606445, height: 0.6833343505859375, depth: 0.0, italic: 0.10162067413330078 }), // 0x55
        Some(TfmChar { width: 0.7852277755737305, height: 0.6833343505859375, depth: 0.0, italic: 0.08453655242919922 }), // 0x56
        Some(TfmChar { width: 1.222722053527832, height: 0.6833343505859375, depth: 0.0, italic: 0.08453655242919922 }), // 0x57
        Some(TfmChar { width: 0.8836565017700195, height: 0.6833343505859375, depth: 0.0, italic: 0.14643096923828125 }), // 0x58
        Some(TfmChar { width: 0.823887825012207, height: 0.6833343505859375, depth: 0.09722423553466797, italic: 0.08453655242919922 }), // 0x59
        Some(TfmChar { width: 0.883976936340332, height: 0.6833343505859375, depth: 0.0, italic: 0.08129596710205078 }), // 0x5A
        Some(TfmChar { width: 0.8333282470703125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.8333282470703125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.8333282470703125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.8333282470703125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.8333282470703125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.7685136795043945, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.7685136795043945, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.574070930480957, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.574070930480957, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.574070930480957, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.574070930480957, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.638885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.638885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.5092573165893555, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.5092573165893555, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.37962818145751953, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.638885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.638885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.7685136795043945, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.638885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.37962818145751953, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.9999923706054688, height: 0.051666259765625, depth: 0.948333740234375, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.924067497253418, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 1.02777099609375, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.5416669845581055, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.12962818145751953 }), // 0x73
        Some(TfmChar { width: 0.8333282470703125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.8333282470703125, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.962956428527832, height: 0.7020845413208008, depth: 0.20208454132080078, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.962956428527832, height: 0.7020845413208008, depth: 0.20208454132080078, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.574070930480957, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.574070930480957, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.574070930480957, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.7685136795043945, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.962956428527832, height: 0.6944456100463867, depth: 0.12963104248046875, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.962956428527832, height: 0.6944456100463867, depth: 0.12963104248046875, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.962956428527832, height: 0.6944456100463867, depth: 0.12963104248046875, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.962956428527832, height: 0.6944456100463867, depth: 0.12963104248046875, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x41, 0x30, 0.22684955596923828),
        (0x42, 0x30, 0.16203594207763672),
        (0x43, 0x30, 0.16203594207763672),
        (0x44, 0x30, 0.09722137451171875),
        (0x45, 0x30, 0.12962818145751953),
        (0x46, 0x30, 0.12962818145751953),
        (0x47, 0x30, 0.12962818145751953),
        (0x48, 0x30, 0.12962818145751953),
        (0x49, 0x30, 0.03240680694580078),
        (0x4A, 0x30, 0.1944427490234375),
        (0x4B, 0x30, 0.06481456756591797),
        (0x4C, 0x30, 0.16203594207763672),
        (0x4D, 0x30, 0.16203594207763672),
        (0x4E, 0x30, 0.09722137451171875),
        (0x4F, 0x30, 0.12962818145751953),
        (0x50, 0x30, 0.09722137451171875),
        (0x51, 0x30, 0.12962818145751953),
        (0x52, 0x30, 0.09722137451171875),
        (0x53, 0x30, 0.16203594207763672),
        (0x54, 0x30, 0.03240680694580078),
        (0x55, 0x30, 0.09722137451171875),
        (0x56, 0x30, 0.03240680694580078),
        (0x57, 0x30, 0.09722137451171875),
        (0x58, 0x30, 0.16203594207763672),
        (0x59, 0x30, 0.09722137451171875),
        (0x5A, 0x30, 0.16203594207763672),
    ],
};

/// `cmsy8.tfm`.
#[rustfmt::skip]
pub static CMSY8: TfmFont = TfmFont {
    name: "cmsy8",
    space: 0.0,
    skew_char: Some(0x30),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305553436279297, 1.0625152587890625, 0.0, 0.6963310241699219, 0.4088325500488281, 0.4574432373046875, 0.7491092681884766, 0.3921661376953125, 0.4154167175292969, 0.3529167175292969, 0.28472137451171875, 0.125, 0.25, 0.39583396911621094, 0.0625, 1.4874992370605469, 1.1375007629394531, 0.25],
    chars: &[
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.29514312744140625, height: 0.4566001892089844, depth: -0.043399810791015625, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.5312576293945312, height: 0.46527862548828125, depth: -0.03472137451171875, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.5312576293945312, height: 0.4566001892089844, depth: -0.043399810791015625, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 1.0625152587890625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.5312576293945312, height: 0.4566001892089844, depth: -0.043399810791015625, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.5312576293945312, height: 0.4566001892089844, depth: -0.043399810791015625, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.8264007568359375, height: 0.4834728240966797, depth: -0.016527175903320312, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.8264007568359375, height: 0.4834728240966797, depth: -0.016527175903320312, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.8264007568359375, height: 0.6686115264892578, depth: 0.1686115264892578, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.8264007568359375, height: 0.6686115264892578, depth: 0.1686115264892578, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.8264007568359375, height: 0.6686115264892578, depth: 0.1686115264892578, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.8264007568359375, height: 0.6686115264892578, depth: 0.1686115264892578, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.8264007568359375, height: 0.6686115264892578, depth: 0.1686115264892578, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.8264007568359375, height: 0.6686115264892578, depth: 0.1686115264892578, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.8264007568359375, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.8264007568359375, height: 0.5045680999755859, depth: 0.0045680999755859375, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.8264007568359375, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.8264007568359375, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.0625152587890625, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.0625152587890625, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.8264007568359375, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.8264007568359375, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.0625152587890625, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.0625152587890625, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.8264007568359375, height: 0.4834728240966797, depth: -0.016527175903320312, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.6493148803710938, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.6493148803710938, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.0625152587890625, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.0625152587890625, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 1.0625152587890625, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.8264007568359375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.2881946563720703, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.0625152587890625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.708343505859375, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.708343505859375, height: 0.5631256103515625, depth: 0.0631256103515625, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.9444580078125, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.9444580078125, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.0, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.0, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5902862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5902862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.708343505859375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.7673721313476562, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.7673721313476562, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.8264007568359375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.8264007568359375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.6493148803710938, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.8494529724121094, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.6946620941162109, height: 0.6833324432373047, depth: 0.0, italic: 0.026975631713867188 }), // 0x42
        Some(TfmChar { width: 0.5626468658447266, height: 0.6833324432373047, depth: 0.0, italic: 0.053440093994140625 }), // 0x43
        Some(TfmChar { width: 0.8217487335205078, height: 0.6833324432373047, depth: 0.0, italic: 0.029514312744140625 }), // 0x44
        Some(TfmChar { width: 0.5607719421386719, height: 0.6833324432373047, depth: 0.0, italic: 0.08649063110351562 }), // 0x45
        Some(TfmChar { width: 0.7583389282226562, height: 0.6833324432373047, depth: 0.0, italic: 0.1001739501953125 }), // 0x46
        Some(TfmChar { width: 0.6309814453125, height: 0.6833324432373047, depth: 0.09722328186035156, italic: 0.05767250061035156 }), // 0x47
        Some(TfmChar { width: 0.9042491912841797, height: 0.6833324432373047, depth: 0.0, italic: 0.00118255615234375 }), // 0x48
        Some(TfmChar { width: 0.5854930877685547, height: 0.6833324432373047, depth: 0.0, italic: 0.06805801391601562 }), // 0x49
        Some(TfmChar { width: 0.7201499938964844, height: 0.6833324432373047, depth: 0.09722328186035156, italic: 0.1855907440185547 }), // 0x4A
        Some(TfmChar { width: 0.8074455261230469, height: 0.6833324432373047, depth: 0.0, italic: 0.006805419921875 }), // 0x4B
        Some(TfmChar { width: 0.7307033538818359, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.2645206451416016, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.8691082000732422, height: 0.6833324432373047, depth: 0.0, italic: 0.14909744262695312 }), // 0x4E
        Some(TfmChar { width: 0.8416080474853516, height: 0.6833324432373047, depth: 0.0, italic: 0.029514312744140625 }), // 0x4F
        Some(TfmChar { width: 0.7433109283447266, height: 0.6833324432373047, depth: 0.0, italic: 0.08308982849121094 }), // 0x50
        Some(TfmChar { width: 0.8677215576171875, height: 0.6833324432373047, depth: 0.09722328186035156, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.9068889617919922, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.6434116363525391, height: 0.6833324432373047, depth: 0.0, italic: 0.07327842712402344 }), // 0x53
        Some(TfmChar { width: 0.5863189697265625, height: 0.6833324432373047, depth: 0.0, italic: 0.25937652587890625 }), // 0x54
        Some(TfmChar { width: 0.6628170013427734, height: 0.6833324432373047, depth: 0.0, italic: 0.1001739501953125 }), // 0x55
        Some(TfmChar { width: 0.6561927795410156, height: 0.6833324432373047, depth: 0.0, italic: 0.08308982849121094 }), // 0x56
        Some(TfmChar { width: 1.054636001586914, height: 0.6833324432373047, depth: 0.0, italic: 0.08308982849121094 }), // 0x57
        Some(TfmChar { width: 0.7563591003417969, height: 0.6833324432373047, depth: 0.0, italic: 0.14642906188964844 }), // 0x58
        Some(TfmChar { width: 0.7058429718017578, height: 0.6833324432373047, depth: 0.09722328186035156, italic: 0.08308982849121094 }), // 0x59
        Some(TfmChar { width: 0.7636222839355469, height: 0.6833324432373047, depth: 0.0, italic: 0.08013725280761719 }), // 0x5A
        Some(TfmChar { width: 0.708343505859375, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.708343505859375, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.708343505859375, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.708343505859375, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.708343505859375, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.6493148803710938, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.6493148803710938, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.47222900390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.47222900390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.47222900390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.47222900390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.41320037841796875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.41320037841796875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.29514312744140625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.6493148803710938, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.29514312744140625, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.8854293823242188, height: 0.04500007629394531, depth: 0.9549999237060547, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.8854293823242188, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.44358253479003906, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.1180572509765625 }), // 0x73
        Some(TfmChar { width: 0.708343505859375, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.708343505859375, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.8264007568359375, height: 0.6686115264892578, depth: 0.1686115264892578, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.8264007568359375, height: 0.6686115264892578, depth: 0.1686115264892578, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.47222900390625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.47222900390625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.47222900390625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.6493148803710938, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.8264007568359375, height: 0.6944446563720703, depth: 0.12962913513183594, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.8264007568359375, height: 0.6944446563720703, depth: 0.12962913513183594, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.8264007568359375, height: 0.6944446563720703, depth: 0.12962913513183594, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.8264007568359375, height: 0.6944446563720703, depth: 0.12962913513183594, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x41, 0x30, 0.20660018920898438),
        (0x42, 0x30, 0.14757156372070312),
        (0x43, 0x30, 0.14757156372070312),
        (0x44, 0x30, 0.08854293823242188),
        (0x45, 0x30, 0.1180572509765625),
        (0x46, 0x30, 0.1180572509765625),
        (0x47, 0x30, 0.1180572509765625),
        (0x48, 0x30, 0.1180572509765625),
        (0x49, 0x30, 0.029514312744140625),
        (0x4A, 0x30, 0.17708587646484375),
        (0x4B, 0x30, 0.05902862548828125),
        (0x4C, 0x30, 0.14757156372070312),
        (0x4D, 0x30, 0.14757156372070312),
        (0x4E, 0x30, 0.08854293823242188),
        (0x4F, 0x30, 0.1180572509765625),
        (0x50, 0x30, 0.08854293823242188),
        (0x51, 0x30, 0.1180572509765625),
        (0x52, 0x30, 0.08854293823242188),
        (0x53, 0x30, 0.14757156372070312),
        (0x54, 0x30, 0.029514312744140625),
        (0x55, 0x30, 0.08854293823242188),
        (0x56, 0x30, 0.029514312744140625),
        (0x57, 0x30, 0.08854293823242188),
        (0x58, 0x30, 0.14757156372070312),
        (0x59, 0x30, 0.08854293823242188),
        (0x5A, 0x30, 0.14757156372070312),
    ],
};

/// `cmsy9.tfm`.
#[rustfmt::skip]
pub static CMSY9: TfmFont = TfmFont {
    name: "cmsy9",
    space: 0.0,
    skew_char: Some(0x30),
    params: &[0.25, 0.0, 0.0, 0.0, 0.4305553436279297, 1.02777099609375, 0.0, 0.6467390060424805, 0.4097023010253906, 0.4498252868652344, 0.67822265625, 0.33933544158935547, 0.48036670684814453, 0.42481136322021484, 0.2870368957519531, 0.11111068725585938, 0.1759319305419922, 0.3518562316894531, 0.05555534362792969, 2.65555477142334, 1.0111122131347656, 0.25],
    chars: &[
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.285491943359375, height: 0.4498443603515625, depth: -0.0501556396484375, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.513885498046875, height: 0.46527767181396484, depth: -0.034722328186035156, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.513885498046875, height: 0.4498443603515625, depth: -0.0501556396484375, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 1.02777099609375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.513885498046875, height: 0.4498443603515625, depth: -0.0501556396484375, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.513885498046875, height: 0.4498443603515625, depth: -0.0501556396484375, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.79937744140625, height: 0.47347164154052734, depth: -0.026528358459472656, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.79937744140625, height: 0.47347164154052734, depth: -0.026528358459472656, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.79937744140625, height: 0.6521520614624023, depth: 0.15215206146240234, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.79937744140625, height: 0.6521520614624023, depth: 0.15215206146240234, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.79937744140625, height: 0.6521520614624023, depth: 0.15215206146240234, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.79937744140625, height: 0.6521520614624023, depth: 0.15215206146240234, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.79937744140625, height: 0.6521520614624023, depth: 0.15215206146240234, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.79937744140625, height: 0.6521520614624023, depth: 0.15215206146240234, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.79937744140625, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.79937744140625, height: 0.49370670318603516, depth: -0.006293296813964844, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.79937744140625, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.79937744140625, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.02777099609375, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.02777099609375, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.79937744140625, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.79937744140625, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.02777099609375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.02777099609375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.79937744140625, height: 0.47347164154052734, depth: -0.026528358459472656, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.628082275390625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.628082275390625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.02777099609375, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.02777099609375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 1.02777099609375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.79937744140625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.27932071685791016, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.02777099609375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.6851806640625, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.6851806640625, height: 0.5509729385375977, depth: 0.050972938537597656, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.91357421875, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.91357421875, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.0, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.0, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.57098388671875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.57098388671875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.6851806640625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.742279052734375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.742279052734375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.79937744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.79937744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.628082275390625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.8211164474487305, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.6736230850219727, height: 0.6833324432373047, depth: 0.0, italic: 0.028886795043945312 }), // 0x42
        Some(TfmChar { width: 0.542572021484375, height: 0.6833324432373047, depth: 0.0, italic: 0.05616283416748047 }), // 0x43
        Some(TfmChar { width: 0.7937602996826172, height: 0.6833324432373047, depth: 0.0, italic: 0.0285491943359375 }), // 0x44
        Some(TfmChar { width: 0.5424346923828125, height: 0.6833324432373047, depth: 0.0, italic: 0.08813095092773438 }), // 0x45
        Some(TfmChar { width: 0.7363348007202148, height: 0.6833324432373047, depth: 0.0, italic: 0.09969043731689453 }), // 0x46
        Some(TfmChar { width: 0.6109075546264648, height: 0.6833324432373047, depth: 0.09722232818603516, italic: 0.0585784912109375 }), // 0x47
        Some(TfmChar { width: 0.8710479736328125, height: 0.6833324432373047, depth: 0.0, italic: 0.0058917999267578125 }), // 0x48
        Some(TfmChar { width: 0.562718391418457, height: 0.6833324432373047, depth: 0.0, italic: 0.07126235961914062 }), // 0x49
        Some(TfmChar { width: 0.696599006652832, height: 0.6833324432373047, depth: 0.09722232818603516, italic: 0.18510913848876953 }), // 0x4A
        Some(TfmChar { width: 0.7821588516235352, height: 0.6833324432373047, depth: 0.0, italic: 0.011054039001464844 }), // 0x4B
        Some(TfmChar { width: 0.7079277038574219, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.2291603088378906, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.8420848846435547, height: 0.6833324432373047, depth: 0.0, italic: 0.14813232421875 }), // 0x4E
        Some(TfmChar { width: 0.8163213729858398, height: 0.6833324432373047, depth: 0.0, italic: 0.0285491943359375 }), // 0x4F
        Some(TfmChar { width: 0.7167701721191406, height: 0.6833324432373047, depth: 0.0, italic: 0.08260726928710938 }), // 0x50
        Some(TfmChar { width: 0.8393449783325195, height: 0.6833324432373047, depth: 0.09722232818603516, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.8738813400268555, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.6223716735839844, height: 0.6833324432373047, depth: 0.0, italic: 0.07423210144042969 }), // 0x53
        Some(TfmChar { width: 0.5631561279296875, height: 0.6833324432373047, depth: 0.0, italic: 0.2564811706542969 }), // 0x54
        Some(TfmChar { width: 0.6422615051269531, height: 0.6833324432373047, depth: 0.0, italic: 0.09969043731689453 }), // 0x55
        Some(TfmChar { width: 0.6320648193359375, height: 0.6833324432373047, depth: 0.0, italic: 0.08260726928710938 }), // 0x56
        Some(TfmChar { width: 1.0174779891967773, height: 0.6833324432373047, depth: 0.0, italic: 0.08260726928710938 }), // 0x57
        Some(TfmChar { width: 0.7324256896972656, height: 0.6833324432373047, depth: 0.0, italic: 0.14642810821533203 }), // 0x58
        Some(TfmChar { width: 0.6849994659423828, height: 0.6833324432373047, depth: 0.09722232818603516, italic: 0.08260726928710938 }), // 0x59
        Some(TfmChar { width: 0.7420024871826172, height: 0.6833324432373047, depth: 0.0, italic: 0.07975292205810547 }), // 0x5A
        Some(TfmChar { width: 0.6851806640625, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.6851806640625, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.6851806640625, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.6851806640625, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.6851806640625, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.628082275390625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.628082275390625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.456787109375, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.456787109375, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.456787109375, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.456787109375, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.399688720703125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.399688720703125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.285491943359375, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.628082275390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.285491943359375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.856475830078125, height: 0.04222297668457031, depth: 0.9577770233154297, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.856475830078125, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.4282398223876953, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.11419677734375 }), // 0x73
        Some(TfmChar { width: 0.6851806640625, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.6851806640625, height: 0.5555572509765625, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.79937744140625, height: 0.6521520614624023, depth: 0.15215206146240234, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.79937744140625, height: 0.6521520614624023, depth: 0.15215206146240234, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.456787109375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.456787109375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.456787109375, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.628082275390625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.79937744140625, height: 0.6944446563720703, depth: 0.12963008880615234, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.79937744140625, height: 0.6944446563720703, depth: 0.12963008880615234, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.79937744140625, height: 0.6944446563720703, depth: 0.12963008880615234, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.79937744140625, height: 0.6944446563720703, depth: 0.12963008880615234, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x41, 0x30, 0.1998443603515625),
        (0x42, 0x30, 0.1427459716796875),
        (0x43, 0x30, 0.1427459716796875),
        (0x44, 0x30, 0.0856475830078125),
        (0x45, 0x30, 0.11419677734375),
        (0x46, 0x30, 0.11419677734375),
        (0x47, 0x30, 0.11419677734375),
        (0x48, 0x30, 0.11419677734375),
        (0x49, 0x30, 0.0285491943359375),
        (0x4A, 0x30, 0.171295166015625),
        (0x4B, 0x30, 0.057098388671875),
        (0x4C, 0x30, 0.1427459716796875),
        (0x4D, 0x30, 0.1427459716796875),
        (0x4E, 0x30, 0.0856475830078125),
        (0x4F, 0x30, 0.11419677734375),
        (0x50, 0x30, 0.0856475830078125),
        (0x51, 0x30, 0.11419677734375),
        (0x52, 0x30, 0.0856475830078125),
        (0x53, 0x30, 0.1427459716796875),
        (0x54, 0x30, 0.0285491943359375),
        (0x55, 0x30, 0.0856475830078125),
        (0x56, 0x30, 0.0285491943359375),
        (0x57, 0x30, 0.0856475830078125),
        (0x58, 0x30, 0.1427459716796875),
        (0x59, 0x30, 0.0856475830078125),
        (0x5A, 0x30, 0.1427459716796875),
    ],
};

/// `cmex10.tfm`.
#[rustfmt::skip]
pub static CMEX10: TfmFont = TfmFont {
    name: "cmex10",
    space: 0.0,
    skew_char: None,
    params: &[0.0, 0.0, 0.0, 0.0, 0.4305553436279297, 1.0000028610229492, 0.0, 0.03999900817871094, 0.11111164093017578, 0.16666698455810547, 0.19999980926513672, 0.6000003814697266, 0.10000038146972656],
    chars: &[
        Some(TfmChar { width: 0.45833587646484375, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.45833587646484375, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.4166688919067383, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.4166688919067383, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.47222423553466797, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.47222423553466797, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.47222423553466797, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.47222423553466797, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.5833358764648438, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.5833358764648438, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.47222423553466797, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.47222423553466797, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.33333396911621094, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.5555572509765625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.5777788162231445, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.5777788162231445, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.597224235534668, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.597224235534668, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.736114501953125, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.736114501953125, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5277805328369141, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5277805328369141, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.5833358764648438, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.5833358764648438, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.5833358764648438, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.5833358764648438, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.7500019073486328, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.7500019073486328, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.7500019073486328, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.7500019073486328, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.0444469451904297, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 1.0444469451904297, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.7916698455810547, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.7916698455810547, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.5833358764648438, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.5833358764648438, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.6388912200927734, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.6388912200927734, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.6388912200927734, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.6388912200927734, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.8055601119995117, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.8055601119995117, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.8055601119995117, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.8055601119995117, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.2777814865112305, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.2777814865112305, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.8111143112182617, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.8111143112182617, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.8750028610229492, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.8750028610229492, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.6666688919067383, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.6666688919067383, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.6666688919067383, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.6666688919067383, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.6666688919067383, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.6666688919067383, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.8888912200927734, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.8888912200927734, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.8888912200927734, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.8888912200927734, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.8888912200927734, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.8888912200927734, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.8888912200927734, height: 0.0, depth: 0.3000030517578125, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.6666688919067383, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.8750028610229492, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.8750028610229492, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.8750028610229492, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.8750028610229492, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.6111125946044922, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6111125946044922, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.8333358764648438, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 1.111114501953125, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.47222328186035156, height: 0.0, depth: 1.1111221313476562, italic: 0.19444561004638672 }), // 0x48
        Some(TfmChar { width: 0.5555572509765625, height: 0.0, depth: 2.2222461700439453, italic: 0.4444456100463867 }), // 0x49
        Some(TfmChar { width: 1.111114501953125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 1.5111160278320312, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 1.111114501953125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.5111160278320312, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 1.111114501953125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 1.5111160278320312, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 1.0555591583251953, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.9444475173950195, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.47222328186035156, height: 0.0, depth: 1.1111221313476562, italic: 0.19444561004638672 }), // 0x52
        Some(TfmChar { width: 0.8333358764648438, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.8333358764648438, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.8333358764648438, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.8333358764648438, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 0.8333358764648438, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 1.444448471069336, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 1.2777814865112305, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.5555572509765625, height: 0.0, depth: 2.2222461700439453, italic: 0.4444456100463867 }), // 0x5A
        Some(TfmChar { width: 1.111114501953125, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 1.111114501953125, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 1.111114501953125, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.111114501953125, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.111114501953125, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.9444475173950195, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 1.2777814865112305, height: 0.10000133514404297, depth: 1.5000123977661133, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.5555572509765625, height: 0.7222232818603516, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 1.0000028610229492, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 1.444448471069336, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.5555572509765625, height: 0.7222232818603516, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 1.0000028610229492, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 1.444448471069336, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.47222423553466797, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.47222423553466797, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.5277805328369141, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5277805328369141, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.5277805328369141, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.5277805328369141, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.6666688919067383, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.6666688919067383, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 1.0000028610229492, height: 0.03999900817871094, depth: 1.160013198852539, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 1.0000028610229492, height: 0.03999900817871094, depth: 1.760019302368164, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 1.0000028610229492, height: 0.03999900817871094, depth: 2.360025405883789, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 1.0000028610229492, height: 0.03999900817871094, depth: 2.960031509399414, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 1.0555591583251953, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 1.0555591583251953, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 1.0555591583251953, height: 0.03999900817871094, depth: 0.5600070953369141, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.7777805328369141, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.6666688919067383, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.6666688919067383, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.45000457763671875, height: 0.11999797821044922, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.45000457763671875, height: 0.11999797821044922, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.45000457763671875, height: 0.11999797821044922, depth: 0.0, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.45000457763671875, height: 0.11999797821044922, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.7777805328369141, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.7777805328369141, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `cmex7.tfm`.
#[rustfmt::skip]
pub static CMEX7: TfmFont = TfmFont {
    name: "cmex7",
    space: 0.0,
    skew_char: None,
    params: &[0.0, 0.0, 0.0, 0.0, 0.4305553436279297, 1.1706409454345703, 0.0, 0.04857063293457031, 0.11110973358154297, 0.16666698455810547, 0.1999988555908203, 0.611109733581543, 0.14285755157470703],
    chars: &[
        Some(TfmChar { width: 0.5391912460327148, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.5391912460327148, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.4930591583251953, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.4930591583251953, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.5545673370361328, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.5545673370361328, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.5545673370361328, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.5545673370361328, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.6775836944580078, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.6775836944580078, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.5545673370361328, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.5545673370361328, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.40079593658447266, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.6468286514282227, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.6396856307983398, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.6396856307983398, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.6929607391357422, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.6929607391357422, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.8467321395874023, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.8467321395874023, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.6160755157470703, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.6160755157470703, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6775836944580078, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.6775836944580078, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.6775836944580078, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.6775836944580078, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.8621063232421875, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.8621063232421875, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.8621063232421875, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.8621063232421875, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.1563549041748047, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 1.1563549041748047, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.9082403182983398, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.9082403182983398, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.6775836944580078, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.6775836944580078, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.7390918731689453, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.7390918731689453, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.7390918731689453, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.7390918731689453, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.923619270324707, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.923619270324707, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.923619270324707, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.923619270324707, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.4146881103515625, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.4146881103515625, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.8980188369750977, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.8980188369750977, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 1.0005016326904297, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.0005016326904297, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.7698450088500977, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.7698450088500977, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.7698450088500977, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.7698450088500977, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.7698450088500977, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.7698450088500977, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 1.0158777236938477, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 1.0158777236938477, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 1.0158777236938477, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 1.0158777236938477, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 1.0158777236938477, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 1.0158777236938477, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 1.0158777236938477, height: 0.0, depth: 0.3000030517578125, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.7698450088500977, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 1.0005016326904297, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 1.0005016326904297, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 1.0005016326904297, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 1.0005016326904297, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.7083368301391602, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.7083368301391602, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.9543695449829102, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 1.2619104385375977, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.5545654296875, height: 0.0, depth: 1.1111230850219727, italic: 0.21527957916259766 }), // 0x48
        Some(TfmChar { width: 0.6468286514282227, height: 0.0, depth: 2.2222461700439453, italic: 0.4920654296875 }), // 0x49
        Some(TfmChar { width: 1.2619104385375977, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 1.704768180847168, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 1.2619104385375977, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.704768180847168, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 1.2619104385375977, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 1.704768180847168, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 1.2004022598266602, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 1.0773859024047852, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.5545654296875, height: 0.0, depth: 1.1111230850219727, italic: 0.21527957916259766 }), // 0x52
        Some(TfmChar { width: 0.9543695449829102, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.9543695449829102, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.9543695449829102, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.9543695449829102, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 0.9543695449829102, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 1.6309595108032227, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 1.4464349746704102, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.6468286514282227, height: 0.0, depth: 2.2222461700439453, italic: 0.4920654296875 }), // 0x5A
        Some(TfmChar { width: 1.2619104385375977, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 1.2619104385375977, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 1.2619104385375977, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.2619104385375977, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.2619104385375977, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 1.0773859024047852, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 1.4464349746704102, height: 0.10000133514404297, depth: 1.5000133514404297, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.6468286514282227, height: 0.7222223281860352, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 1.1388940811157227, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 1.6309595108032227, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.6468286514282227, height: 0.7222223281860352, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 1.1388940811157227, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 1.6309595108032227, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.5545673370361328, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.5545673370361328, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.6160755157470703, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.6160755157470703, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.6160755157470703, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.6160755157470703, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.7698450088500977, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.7698450088500977, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 1.123021125793457, height: 0.04857063293457031, depth: 1.1514415740966797, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 1.123021125793457, height: 0.04857063293457031, depth: 1.7514476776123047, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 1.123021125793457, height: 0.04857063293457031, depth: 2.3514537811279297, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 1.123021125793457, height: 0.04857063293457031, depth: 2.9514598846435547, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 1.1845293045043945, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 1.1845293045043945, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 1.1845293045043945, height: 0.04857063293457031, depth: 0.5514354705810547, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.8928613662719727, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.7698450088500977, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.7698450088500977, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.4817523956298828, height: 0.14571285247802734, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.4817523956298828, height: 0.14571285247802734, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.4817523956298828, height: 0.14571285247802734, depth: 0.0, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.4817523956298828, height: 0.14571285247802734, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.8928613662719727, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.8928613662719727, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `cmex8.tfm`.
#[rustfmt::skip]
pub static CMEX8: TfmFont = TfmFont {
    name: "cmex8",
    space: 0.0,
    skew_char: None,
    params: &[0.0, 0.0, 0.0, 0.0, 0.4305553436279297, 1.0625152587890625, 0.0, 0.04500007629394531, 0.11111068725585938, 0.16666603088378906, 0.20000076293945312, 0.6076393127441406, 0.125],
    chars: &[
        Some(TfmChar { width: 0.4869861602783203, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.4869861602783203, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.4427146911621094, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.4427146911621094, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.5017433166503906, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.5017433166503906, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.5017433166503906, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.5017433166503906, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.6198005676269531, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.6198005676269531, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.5017433166503906, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.5017433166503906, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.3541717529296875, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.5902862548828125, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.6138973236083984, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.6138973236083984, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.6345577239990234, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.6345577239990234, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.7821292877197266, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.7821292877197266, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5607719421386719, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5607719421386719, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6198005676269531, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.6198005676269531, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.6198005676269531, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.6198005676269531, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.7968864440917969, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.7968864440917969, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.7968864440917969, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.7968864440917969, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.1097373962402344, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 1.1097373962402344, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.8411579132080078, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.8411579132080078, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.6198005676269531, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.6198005676269531, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.6788291931152344, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.6788291931152344, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.6788291931152344, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.6788291931152344, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.8559150695800781, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.8559150695800781, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.8559150695800781, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.8559150695800781, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.3576583862304688, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.3576583862304688, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.8618183135986328, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.8618183135986328, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.9297008514404297, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.9297008514404297, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.708343505859375, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.708343505859375, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.708343505859375, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.708343505859375, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.708343505859375, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.708343505859375, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.9444580078125, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.9444580078125, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.9444580078125, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.9444580078125, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.9444580078125, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.9444580078125, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.9444580078125, height: 0.0, depth: 0.3000030517578125, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.708343505859375, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.9297008514404297, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.9297008514404297, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.9297008514404297, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.9297008514404297, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.6493148803710938, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6493148803710938, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.8854293823242188, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 1.180572509765625, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.5017433166503906, height: 0.0, depth: 1.1111221313476562, italic: 0.20660018920898438 }), // 0x48
        Some(TfmChar { width: 0.5902862548828125, height: 0.0, depth: 2.2222461700439453, italic: 0.47222900390625 }), // 0x49
        Some(TfmChar { width: 1.180572509765625, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 1.6055793762207031, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 1.180572509765625, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.6055793762207031, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 1.180572509765625, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 1.6055793762207031, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 1.1215438842773438, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 1.0034866333007812, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.5017433166503906, height: 0.0, depth: 1.1111221313476562, italic: 0.20660018920898438 }), // 0x52
        Some(TfmChar { width: 0.8854293823242188, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.8854293823242188, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.8854293823242188, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.8854293823242188, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 0.8854293823242188, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 1.5347442626953125, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 1.3576583862304688, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.5902862548828125, height: 0.0, depth: 2.2222461700439453, italic: 0.47222900390625 }), // 0x5A
        Some(TfmChar { width: 1.180572509765625, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 1.180572509765625, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 1.180572509765625, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.180572509765625, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.180572509765625, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 1.0034866333007812, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 1.3576583862304688, height: 0.10000038146972656, depth: 1.5000114440917969, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.5902862548828125, height: 0.7222232818603516, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 1.0625152587890625, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 1.5347442626953125, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.5902862548828125, height: 0.7222232818603516, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 1.0625152587890625, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 1.5347442626953125, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.5017433166503906, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.5017433166503906, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.5607719421386719, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5607719421386719, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.5607719421386719, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.5607719421386719, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.708343505859375, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.708343505859375, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 1.0625152587890625, height: 0.04500007629394531, depth: 1.1550121307373047, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 1.0625152587890625, height: 0.04500007629394531, depth: 1.7550182342529297, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 1.0625152587890625, height: 0.04500007629394531, depth: 2.3550243377685547, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 1.0625152587890625, height: 0.04500007629394531, depth: 2.9550304412841797, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 1.1215438842773438, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 1.1215438842773438, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 1.1215438842773438, height: 0.04500007629394531, depth: 0.5550060272216797, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.8264007568359375, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.708343505859375, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.708343505859375, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.45000457763671875, height: 0.13500022888183594, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.45000457763671875, height: 0.13500022888183594, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.45000457763671875, height: 0.13500022888183594, depth: 0.0, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.45000457763671875, height: 0.13500022888183594, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.8264007568359375, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.8264007568359375, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `cmex9.tfm`.
#[rustfmt::skip]
pub static CMEX9: TfmFont = TfmFont {
    name: "cmex9",
    space: 0.0,
    skew_char: None,
    params: &[0.0, 0.0, 0.0, 0.0, 0.4305553436279297, 1.02777099609375, 0.0, 0.04222297668457031, 0.11111068725585938, 0.16666698455810547, 0.20000076293945312, 0.6049385070800781, 0.11111068725585938],
    chars: &[
        Some(TfmChar { width: 0.47106075286865234, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.47106075286865234, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.4282379150390625, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.4282379150390625, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.4853363037109375, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.4853363037109375, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.4853363037109375, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.4853363037109375, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.5995349884033203, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.5995349884033203, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.4853382110595703, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.4853382110595703, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.34259033203125, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.57098388671875, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.5938224792480469, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.5938224792480469, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.6138067245483398, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.6138067245483398, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.7565526962280273, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.7565526962280273, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5424346923828125, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5424346923828125, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.5995330810546875, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.5995330810546875, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.5995330810546875, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.5995330810546875, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.7708301544189453, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.7708301544189453, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.7708301544189453, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.7708301544189453, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.073451042175293, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 1.073451042175293, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.8136510848999023, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.8136510848999023, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.5995330810546875, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.5995330810546875, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.6566314697265625, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.6566314697265625, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.6566314697265625, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.6566314697265625, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.8279266357421875, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.8279266357421875, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.8279266357421875, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.8279266357421875, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 1.313262939453125, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.313262939453125, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.8336372375488281, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.8336372375488281, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.8992986679077148, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.8992986679077148, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.6851806640625, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.6851806640625, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.6851806640625, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.6851806640625, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.6851806640625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.6851806640625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.91357421875, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.91357421875, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.91357421875, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.91357421875, height: 0.0, depth: 0.9000091552734375, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.91357421875, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.91357421875, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.91357421875, height: 0.0, depth: 0.3000030517578125, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.6851806640625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.8992986679077148, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.8992986679077148, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.8992986679077148, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.8992986679077148, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.628082275390625, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.628082275390625, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.856475830078125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 1.1419677734375, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.4853363037109375, height: 0.0, depth: 1.1111230850219727, italic: 0.1998443603515625 }), // 0x48
        Some(TfmChar { width: 0.57098388671875, height: 0.0, depth: 2.2222461700439453, italic: 0.456787109375 }), // 0x49
        Some(TfmChar { width: 1.1419677734375, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 1.553074836730957, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 1.1419677734375, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.553074836730957, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 1.1419677734375, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 1.553074836730957, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 1.084869384765625, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.970672607421875, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.4853363037109375, height: 0.0, depth: 1.1111230850219727, italic: 0.1998443603515625 }), // 0x52
        Some(TfmChar { width: 0.856475830078125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.856475830078125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.856475830078125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.856475830078125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 0.856475830078125, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 1.48455810546875, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 1.313262939453125, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.57098388671875, height: 0.0, depth: 2.2222461700439453, italic: 0.456787109375 }), // 0x5A
        Some(TfmChar { width: 1.1419677734375, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 1.1419677734375, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 1.1419677734375, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.1419677734375, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.1419677734375, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.970672607421875, height: 0.0, depth: 1.0000133514404297, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 1.313262939453125, height: 0.10000133514404297, depth: 1.5000114440917969, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.57098388671875, height: 0.7222223281860352, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 1.02777099609375, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 1.48455810546875, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.57098388671875, height: 0.7222223281860352, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 1.02777099609375, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 1.48455810546875, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.4853363037109375, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.4853363037109375, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.5424346923828125, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5424346923828125, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.5424346923828125, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.5424346923828125, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.6851806640625, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.6851806640625, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 1.02777099609375, height: 0.04222297668457031, depth: 1.1577892303466797, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 1.02777099609375, height: 0.04222297668457031, depth: 1.7577953338623047, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 1.02777099609375, height: 0.04222297668457031, depth: 2.3578014373779297, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 1.02777099609375, height: 0.04222297668457031, depth: 2.9578075408935547, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 1.084869384765625, height: 0.0, depth: 1.800018310546875, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 1.084869384765625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 1.084869384765625, height: 0.04222297668457031, depth: 0.5577831268310547, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.79937744140625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.6851806640625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.6851806640625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.45000553131103516, height: 0.12666797637939453, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.45000553131103516, height: 0.12666797637939453, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.45000553131103516, height: 0.12666797637939453, depth: 0.0, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.45000553131103516, height: 0.12666797637939453, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.79937744140625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.79937744140625, height: 0.0, depth: 0.600006103515625, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `cmr10.tfm`.
#[rustfmt::skip]
pub static CMR10: TfmFont = TfmFont {
    name: "cmr10",
    space: 0.33333396911621094,
    skew_char: None,
    params: &[0.0, 0.33333396911621094, 0.16666698455810547, 0.11111164093017578, 0.4305553436279297, 1.0000028610229492, 0.11111164093017578],
    chars: &[
        Some(TfmChar { width: 0.6250019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.8333358764648438, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.7777805328369141, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.6944456100463867, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.6666688919067383, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.722224235534668, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.7777805328369141, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.722224235534668, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.7777805328369141, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.722224235534668, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.5833358764648438, height: 0.6944446563720703, depth: 0.0, italic: 0.07777881622314453 }), // 0x0B
        Some(TfmChar { width: 0.5555572509765625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.5555572509765625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.8333358764648438, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.8333358764648438, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.27777862548828125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.3055572509765625, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5000019073486328, height: 0.6284732818603516, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.5000019073486328, height: 0.5677766799926758, depth: 0.0, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.7500019073486328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.4444456100463867, height: 0.0, depth: 0.17013835906982422, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.5000028610229492, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.722224235534668, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.7777805328369141, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.5000019073486328, height: 0.5277786254882812, depth: 0.09722328186035156, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.9027805328369141, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.0138912200927734, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.7777805328369141, height: 0.7319440841674805, depth: 0.04861164093017578, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.27777862548828125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.27777862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.8333358764648438, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.8333358764648438, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.7777805328369141, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.27777862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.38889026641845703, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.38889026641845703, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.7777805328369141, height: 0.5833339691162109, depth: 0.08333396911621094, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.27777862548828125, height: 0.10555553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.33333396911621094, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.27777862548828125, height: 0.10555553436279297, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.5000019073486328, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5000019073486328, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.27777862548828125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.27777862548828125, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.27777862548828125, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.7777805328369141, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.47222423553466797, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.47222423553466797, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.7777805328369141, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.7083358764648438, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.722224235534668, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.7638912200927734, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6805572509765625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6527805328369141, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.784724235534668, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.3611116409301758, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.513890266418457, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.7777805328369141, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.6250019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.9166688919067383, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.7777805328369141, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.6805572509765625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.7777805328369141, height: 0.6833324432373047, depth: 0.1944446563720703, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7361125946044922, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.5555572509765625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.722224235534668, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.013888359069824219 }), // 0x56
        Some(TfmChar { width: 1.027780532836914, height: 0.6833324432373047, depth: 0.0, italic: 0.013888359069824219 }), // 0x57
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.7500019073486328, height: 0.6833324432373047, depth: 0.0, italic: 0.024999618530273438 }), // 0x59
        Some(TfmChar { width: 0.6111125946044922, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.27777862548828125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.27777862548828125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.27777862548828125, height: 0.6678590774536133, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.27777862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.5555572509765625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.4444456100463867, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.5555572509765625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.4444456100463867, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.3055572509765625, height: 0.6944446563720703, depth: 0.0, italic: 0.07777881622314453 }), // 0x66
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.013888359069824219 }), // 0x67
        Some(TfmChar { width: 0.5555572509765625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.27777862548828125, height: 0.6678590774536133, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.3055572509765625, height: 0.6678590774536133, depth: 0.1944446563720703, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5277805328369141, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.27777862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.8333358764648438, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.5555572509765625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5555572509765625, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.5277786254882812, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.39166831970214844, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.39444541931152344, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.38889026641845703, height: 0.6150798797607422, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.5555572509765625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.5277805328369141, height: 0.4305553436279297, depth: 0.0, italic: 0.013888359069824219 }), // 0x76
        Some(TfmChar { width: 0.722224235534668, height: 0.4305553436279297, depth: 0.0, italic: 0.013888359069824219 }), // 0x77
        Some(TfmChar { width: 0.5277805328369141, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5277805328369141, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.013888359069824219 }), // 0x79
        Some(TfmChar { width: 0.4444456100463867, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.5000019073486328, height: 0.4305553436279297, depth: 0.0, italic: 0.02777862548828125 }), // 0x7B
        Some(TfmChar { width: 1.0000028610229492, height: 0.4305553436279297, depth: 0.0, italic: 0.02777862548828125 }), // 0x7C
        Some(TfmChar { width: 0.5000019073486328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.5000019073486328, height: 0.6678590774536133, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.5000019073486328, height: 0.6678590774536133, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x0B, 0x21, 0.07777881622314453),
        (0x0B, 0x27, 0.07777881622314453),
        (0x0B, 0x29, 0.07777881622314453),
        (0x0B, 0x3F, 0.07777881622314453),
        (0x0B, 0x5D, 0.07777881622314453),
        (0x20, 0x4C, -0.3194456100463867),
        (0x20, 0x6C, -0.27777862548828125),
        (0x27, 0x21, 0.11111164093017578),
        (0x27, 0x3F, 0.11111164093017578),
        (0x41, 0x43, -0.02777862548828125),
        (0x41, 0x47, -0.02777862548828125),
        (0x41, 0x4F, -0.02777862548828125),
        (0x41, 0x51, -0.02777862548828125),
        (0x41, 0x54, -0.08333396911621094),
        (0x41, 0x55, -0.02777862548828125),
        (0x41, 0x56, -0.11111164093017578),
        (0x41, 0x57, -0.11111164093017578),
        (0x41, 0x59, -0.08333396911621094),
        (0x41, 0x74, -0.02777862548828125),
        (0x44, 0x41, -0.02777862548828125),
        (0x44, 0x56, -0.02777862548828125),
        (0x44, 0x57, -0.02777862548828125),
        (0x44, 0x58, -0.02777862548828125),
        (0x44, 0x59, -0.02777862548828125),
        (0x46, 0x41, -0.11111164093017578),
        (0x46, 0x43, -0.02777862548828125),
        (0x46, 0x47, -0.02777862548828125),
        (0x46, 0x4F, -0.02777862548828125),
        (0x46, 0x51, -0.02777862548828125),
        (0x46, 0x61, -0.08333396911621094),
        (0x46, 0x65, -0.08333396911621094),
        (0x46, 0x6F, -0.08333396911621094),
        (0x46, 0x72, -0.08333396911621094),
        (0x46, 0x75, -0.08333396911621094),
        (0x49, 0x49, 0.02777862548828125),
        (0x4B, 0x43, -0.02777862548828125),
        (0x4B, 0x47, -0.02777862548828125),
        (0x4B, 0x4F, -0.02777862548828125),
        (0x4B, 0x51, -0.02777862548828125),
        (0x4C, 0x54, -0.08333396911621094),
        (0x4C, 0x56, -0.11111164093017578),
        (0x4C, 0x57, -0.11111164093017578),
        (0x4C, 0x59, -0.08333396911621094),
        (0x4F, 0x41, -0.02777862548828125),
        (0x4F, 0x56, -0.02777862548828125),
        (0x4F, 0x57, -0.02777862548828125),
        (0x4F, 0x58, -0.02777862548828125),
        (0x4F, 0x59, -0.02777862548828125),
        (0x50, 0x2C, -0.08333396911621094),
        (0x50, 0x2E, -0.08333396911621094),
        (0x50, 0x41, -0.08333396911621094),
        (0x50, 0x61, -0.02777862548828125),
        (0x50, 0x65, -0.02777862548828125),
        (0x50, 0x6F, -0.02777862548828125),
        (0x52, 0x43, -0.02777862548828125),
        (0x52, 0x47, -0.02777862548828125),
        (0x52, 0x4F, -0.02777862548828125),
        (0x52, 0x51, -0.02777862548828125),
        (0x52, 0x54, -0.08333396911621094),
        (0x52, 0x55, -0.02777862548828125),
        (0x52, 0x56, -0.11111164093017578),
        (0x52, 0x57, -0.11111164093017578),
        (0x52, 0x59, -0.08333396911621094),
        (0x52, 0x74, -0.02777862548828125),
        (0x54, 0x41, -0.08333396911621094),
        (0x54, 0x61, -0.08333396911621094),
        (0x54, 0x65, -0.08333396911621094),
        (0x54, 0x6F, -0.08333396911621094),
        (0x54, 0x72, -0.08333396911621094),
        (0x54, 0x75, -0.08333396911621094),
        (0x54, 0x79, -0.02777862548828125),
        (0x56, 0x41, -0.11111164093017578),
        (0x56, 0x43, -0.02777862548828125),
        (0x56, 0x47, -0.02777862548828125),
        (0x56, 0x4F, -0.02777862548828125),
        (0x56, 0x51, -0.02777862548828125),
        (0x56, 0x61, -0.08333396911621094),
        (0x56, 0x65, -0.08333396911621094),
        (0x56, 0x6F, -0.08333396911621094),
        (0x56, 0x72, -0.08333396911621094),
        (0x56, 0x75, -0.08333396911621094),
        (0x57, 0x41, -0.11111164093017578),
        (0x57, 0x43, -0.02777862548828125),
        (0x57, 0x47, -0.02777862548828125),
        (0x57, 0x4F, -0.02777862548828125),
        (0x57, 0x51, -0.02777862548828125),
        (0x57, 0x61, -0.08333396911621094),
        (0x57, 0x65, -0.08333396911621094),
        (0x57, 0x6F, -0.08333396911621094),
        (0x57, 0x72, -0.08333396911621094),
        (0x57, 0x75, -0.08333396911621094),
        (0x58, 0x43, -0.02777862548828125),
        (0x58, 0x47, -0.02777862548828125),
        (0x58, 0x4F, -0.02777862548828125),
        (0x58, 0x51, -0.02777862548828125),
        (0x59, 0x41, -0.08333396911621094),
        (0x59, 0x61, -0.08333396911621094),
        (0x59, 0x65, -0.08333396911621094),
        (0x59, 0x6F, -0.08333396911621094),
        (0x59, 0x72, -0.08333396911621094),
        (0x59, 0x75, -0.08333396911621094),
        (0x61, 0x6A, 0.05555534362792969),
        (0x61, 0x76, -0.02777862548828125),
        (0x61, 0x77, -0.02777862548828125),
        (0x61, 0x79, -0.02777862548828125),
        (0x62, 0x63, 0.02777862548828125),
        (0x62, 0x64, 0.02777862548828125),
        (0x62, 0x65, 0.02777862548828125),
        (0x62, 0x6A, 0.05555534362792969),
        (0x62, 0x6F, 0.02777862548828125),
        (0x62, 0x71, 0.02777862548828125),
        (0x62, 0x76, -0.02777862548828125),
        (0x62, 0x77, -0.02777862548828125),
        (0x62, 0x78, -0.02777862548828125),
        (0x62, 0x79, -0.02777862548828125),
        (0x63, 0x68, -0.02777862548828125),
        (0x63, 0x6B, -0.02777862548828125),
        (0x66, 0x21, 0.07777881622314453),
        (0x66, 0x27, 0.07777881622314453),
        (0x66, 0x29, 0.07777881622314453),
        (0x66, 0x3F, 0.07777881622314453),
        (0x66, 0x5D, 0.07777881622314453),
        (0x67, 0x6A, 0.02777862548828125),
        (0x68, 0x62, -0.02777862548828125),
        (0x68, 0x74, -0.02777862548828125),
        (0x68, 0x75, -0.02777862548828125),
        (0x68, 0x76, -0.02777862548828125),
        (0x68, 0x77, -0.02777862548828125),
        (0x68, 0x79, -0.02777862548828125),
        (0x6B, 0x61, -0.05555534362792969),
        (0x6B, 0x63, -0.02777862548828125),
        (0x6B, 0x65, -0.02777862548828125),
        (0x6B, 0x6F, -0.02777862548828125),
        (0x6D, 0x62, -0.02777862548828125),
        (0x6D, 0x74, -0.02777862548828125),
        (0x6D, 0x75, -0.02777862548828125),
        (0x6D, 0x76, -0.02777862548828125),
        (0x6D, 0x77, -0.02777862548828125),
        (0x6D, 0x79, -0.02777862548828125),
        (0x6E, 0x62, -0.02777862548828125),
        (0x6E, 0x74, -0.02777862548828125),
        (0x6E, 0x75, -0.02777862548828125),
        (0x6E, 0x76, -0.02777862548828125),
        (0x6E, 0x77, -0.02777862548828125),
        (0x6E, 0x79, -0.02777862548828125),
        (0x6F, 0x63, 0.02777862548828125),
        (0x6F, 0x64, 0.02777862548828125),
        (0x6F, 0x65, 0.02777862548828125),
        (0x6F, 0x6A, 0.05555534362792969),
        (0x6F, 0x6F, 0.02777862548828125),
        (0x6F, 0x71, 0.02777862548828125),
        (0x6F, 0x76, -0.02777862548828125),
        (0x6F, 0x77, -0.02777862548828125),
        (0x6F, 0x78, -0.02777862548828125),
        (0x6F, 0x79, -0.02777862548828125),
        (0x70, 0x63, 0.02777862548828125),
        (0x70, 0x64, 0.02777862548828125),
        (0x70, 0x65, 0.02777862548828125),
        (0x70, 0x6A, 0.05555534362792969),
        (0x70, 0x6F, 0.02777862548828125),
        (0x70, 0x71, 0.02777862548828125),
        (0x70, 0x76, -0.02777862548828125),
        (0x70, 0x77, -0.02777862548828125),
        (0x70, 0x78, -0.02777862548828125),
        (0x70, 0x79, -0.02777862548828125),
        (0x74, 0x77, -0.02777862548828125),
        (0x74, 0x79, -0.02777862548828125),
        (0x75, 0x77, -0.02777862548828125),
        (0x76, 0x61, -0.05555534362792969),
        (0x76, 0x63, -0.02777862548828125),
        (0x76, 0x65, -0.02777862548828125),
        (0x76, 0x6F, -0.02777862548828125),
        (0x77, 0x61, -0.02777862548828125),
        (0x77, 0x63, -0.02777862548828125),
        (0x77, 0x65, -0.02777862548828125),
        (0x77, 0x6F, -0.02777862548828125),
        (0x79, 0x2C, -0.08333396911621094),
        (0x79, 0x2E, -0.08333396911621094),
        (0x79, 0x61, -0.02777862548828125),
        (0x79, 0x65, -0.02777862548828125),
        (0x79, 0x6F, -0.02777862548828125),
    ],
};

/// `cmr7.tfm`.
#[rustfmt::skip]
pub static CMR7: TfmFont = TfmFont {
    name: "cmr7",
    space: 0.38492298126220703,
    skew_char: None,
    params: &[0.0, 0.38492298126220703, 0.1845245361328125, 0.123016357421875, 0.4305553436279297, 1.1388940811157227, 0.123016357421875],
    chars: &[
        Some(TfmChar { width: 0.7063531875610352, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.9384965896606445, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.876988410949707, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.7817516326904297, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.753972053527832, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.8432598114013672, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.8154802322387695, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.876988410949707, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.8154802322387695, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.876988410949707, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.8154802322387695, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.6775836944580078, height: 0.6944446563720703, depth: 0.0, italic: 0.07986259460449219 }), // 0x0B
        Some(TfmChar { width: 0.6468286514282227, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.6468286514282227, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.9702434539794922, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.9702434539794922, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.32341480255126953, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.3541698455810547, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.5694475173950195, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.5694475173950195, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5694475173950195, height: 0.6284723281860352, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5694475173950195, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.5694475173950195, height: 0.5742034912109375, depth: 0.0, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.8432598114013672, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.507939338684082, height: 0.0, depth: 0.17014026641845703, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.5694494247436523, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.8154802322387695, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.876988410949707, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.5694475173950195, height: 0.5277776718139648, depth: 0.09722232818603516, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.0138940811157227, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.1369104385375977, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.876988410949707, height: 0.7319469451904297, depth: 0.04861259460449219, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.32341480255126953, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.32341480255126953, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.5694475173950195, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.9384965896606445, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.5694475173950195, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.9384965896606445, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.876988410949707, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.32341480255126953, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.44643115997314453, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.44643115997314453, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.5694475173950195, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.876988410949707, height: 0.619049072265625, depth: 0.119049072265625, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.32341480255126953, height: 0.11507987976074219, depth: 0.1944446563720703, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.38492298126220703, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.32341480255126953, height: 0.11507987976074219, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.5694475173950195, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5694475173950195, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.32341480255126953, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.32341480255126953, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.32341480255126953, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.876988410949707, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.5386943817138672, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.5386943817138672, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.876988410949707, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.8432598114013672, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.7986164093017578, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.8154802322387695, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.8601245880126953, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.7678613662719727, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.7371082305908203, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.8839330673217773, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.8432598114013672, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.4127025604248047, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.5833368301391602, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.8740148544311523, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.7063531875610352, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.0277843475341797, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.8432598114013672, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.876988410949707, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.7678613662719727, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.876988410949707, height: 0.6833343505859375, depth: 0.1944446563720703, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.8293695449829102, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.630955696105957, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.8154802322387695, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.8432598114013672, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.8432598114013672, height: 0.6833343505859375, depth: 0.0, italic: 0.015376091003417969 }), // 0x56
        Some(TfmChar { width: 1.1508007049560547, height: 0.6833343505859375, depth: 0.0, italic: 0.015376091003417969 }), // 0x57
        Some(TfmChar { width: 0.8432598114013672, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.8432598114013672, height: 0.6833343505859375, depth: 0.0, italic: 0.027679443359375 }), // 0x59
        Some(TfmChar { width: 0.6924638748168945, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.32341480255126953, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.5694475173950195, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.32341480255126953, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.5694475173950195, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.32341480255126953, height: 0.6726207733154297, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.32341480255126953, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.5694475173950195, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.630955696105957, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.507939338684082, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.630955696105957, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.507939338684082, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.3541698455810547, height: 0.6944446563720703, depth: 0.0, italic: 0.07986259460449219 }), // 0x66
        Some(TfmChar { width: 0.5694475173950195, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.015376091003417969 }), // 0x67
        Some(TfmChar { width: 0.630955696105957, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.32341480255126953, height: 0.6726207733154297, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.3541698455810547, height: 0.6726207733154297, depth: 0.1944446563720703, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.6002025604248047, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.32341480255126953, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.9384965896606445, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.630955696105957, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.5694475173950195, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.630955696105957, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.6001996994018555, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.44643115997314453, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.45258235931396484, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.44643115997314453, height: 0.6150798797607422, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.630955696105957, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.6002025604248047, height: 0.4305553436279297, depth: 0.0, italic: 0.015376091003417969 }), // 0x76
        Some(TfmChar { width: 0.8154802322387695, height: 0.4305553436279297, depth: 0.0, italic: 0.015376091003417969 }), // 0x77
        Some(TfmChar { width: 0.6002025604248047, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.6002025604248047, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.015376091003417969 }), // 0x79
        Some(TfmChar { width: 0.507939338684082, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.5694475173950195, height: 0.4305553436279297, depth: 0.0, italic: 0.030755043029785156 }), // 0x7B
        Some(TfmChar { width: 1.1388940811157227, height: 0.4305553436279297, depth: 0.0, italic: 0.030755043029785156 }), // 0x7C
        Some(TfmChar { width: 0.5694475173950195, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.5694475173950195, height: 0.6726207733154297, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.5694475173950195, height: 0.6726207733154297, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x0B, 0x21, 0.07986259460449219),
        (0x0B, 0x27, 0.07986259460449219),
        (0x0B, 0x29, 0.07986259460449219),
        (0x0B, 0x3F, 0.07986259460449219),
        (0x0B, 0x5D, 0.07986259460449219),
        (0x20, 0x4C, -0.3680591583251953),
        (0x20, 0x6C, -0.32341480255126953),
        (0x27, 0x21, 0.123016357421875),
        (0x27, 0x3F, 0.123016357421875),
        (0x41, 0x43, -0.030755043029785156),
        (0x41, 0x47, -0.030755043029785156),
        (0x41, 0x4F, -0.030755043029785156),
        (0x41, 0x51, -0.030755043029785156),
        (0x41, 0x54, -0.09226322174072266),
        (0x41, 0x55, -0.030755043029785156),
        (0x41, 0x56, -0.123016357421875),
        (0x41, 0x57, -0.123016357421875),
        (0x41, 0x59, -0.09226322174072266),
        (0x41, 0x74, -0.030755043029785156),
        (0x44, 0x41, -0.030755043029785156),
        (0x44, 0x56, -0.030755043029785156),
        (0x44, 0x57, -0.030755043029785156),
        (0x44, 0x58, -0.030755043029785156),
        (0x44, 0x59, -0.030755043029785156),
        (0x46, 0x41, -0.123016357421875),
        (0x46, 0x43, -0.030755043029785156),
        (0x46, 0x47, -0.030755043029785156),
        (0x46, 0x4F, -0.030755043029785156),
        (0x46, 0x51, -0.030755043029785156),
        (0x46, 0x61, -0.09226322174072266),
        (0x46, 0x65, -0.09226322174072266),
        (0x46, 0x6F, -0.09226322174072266),
        (0x46, 0x72, -0.09226322174072266),
        (0x46, 0x75, -0.09226322174072266),
        (0x49, 0x49, 0.030755043029785156),
        (0x4B, 0x43, -0.030755043029785156),
        (0x4B, 0x47, -0.030755043029785156),
        (0x4B, 0x4F, -0.030755043029785156),
        (0x4B, 0x51, -0.030755043029785156),
        (0x4C, 0x54, -0.09226322174072266),
        (0x4C, 0x56, -0.123016357421875),
        (0x4C, 0x57, -0.123016357421875),
        (0x4C, 0x59, -0.09226322174072266),
        (0x4F, 0x41, -0.030755043029785156),
        (0x4F, 0x56, -0.030755043029785156),
        (0x4F, 0x57, -0.030755043029785156),
        (0x4F, 0x58, -0.030755043029785156),
        (0x4F, 0x59, -0.030755043029785156),
        (0x50, 0x2C, -0.09226322174072266),
        (0x50, 0x2E, -0.09226322174072266),
        (0x50, 0x41, -0.09226322174072266),
        (0x50, 0x61, -0.030755043029785156),
        (0x50, 0x65, -0.030755043029785156),
        (0x50, 0x6F, -0.030755043029785156),
        (0x52, 0x43, -0.030755043029785156),
        (0x52, 0x47, -0.030755043029785156),
        (0x52, 0x4F, -0.030755043029785156),
        (0x52, 0x51, -0.030755043029785156),
        (0x52, 0x54, -0.09226322174072266),
        (0x52, 0x55, -0.030755043029785156),
        (0x52, 0x56, -0.123016357421875),
        (0x52, 0x57, -0.123016357421875),
        (0x52, 0x59, -0.09226322174072266),
        (0x52, 0x74, -0.030755043029785156),
        (0x54, 0x41, -0.09226322174072266),
        (0x54, 0x61, -0.09226322174072266),
        (0x54, 0x65, -0.09226322174072266),
        (0x54, 0x6F, -0.09226322174072266),
        (0x54, 0x72, -0.09226322174072266),
        (0x54, 0x75, -0.09226322174072266),
        (0x54, 0x79, -0.030755043029785156),
        (0x56, 0x41, -0.123016357421875),
        (0x56, 0x43, -0.030755043029785156),
        (0x56, 0x47, -0.030755043029785156),
        (0x56, 0x4F, -0.030755043029785156),
        (0x56, 0x51, -0.030755043029785156),
        (0x56, 0x61, -0.09226322174072266),
        (0x56, 0x65, -0.09226322174072266),
        (0x56, 0x6F, -0.09226322174072266),
        (0x56, 0x72, -0.09226322174072266),
        (0x56, 0x75, -0.09226322174072266),
        (0x57, 0x41, -0.123016357421875),
        (0x57, 0x43, -0.030755043029785156),
        (0x57, 0x47, -0.030755043029785156),
        (0x57, 0x4F, -0.030755043029785156),
        (0x57, 0x51, -0.030755043029785156),
        (0x57, 0x61, -0.09226322174072266),
        (0x57, 0x65, -0.09226322174072266),
        (0x57, 0x6F, -0.09226322174072266),
        (0x57, 0x72, -0.09226322174072266),
        (0x57, 0x75, -0.09226322174072266),
        (0x58, 0x43, -0.030755043029785156),
        (0x58, 0x47, -0.030755043029785156),
        (0x58, 0x4F, -0.030755043029785156),
        (0x58, 0x51, -0.030755043029785156),
        (0x59, 0x41, -0.09226322174072266),
        (0x59, 0x61, -0.09226322174072266),
        (0x59, 0x65, -0.09226322174072266),
        (0x59, 0x6F, -0.09226322174072266),
        (0x59, 0x72, -0.09226322174072266),
        (0x59, 0x75, -0.09226322174072266),
        (0x61, 0x6A, 0.0615081787109375),
        (0x61, 0x76, -0.030755043029785156),
        (0x61, 0x77, -0.030755043029785156),
        (0x61, 0x79, -0.030755043029785156),
        (0x62, 0x63, 0.030755043029785156),
        (0x62, 0x64, 0.030755043029785156),
        (0x62, 0x65, 0.030755043029785156),
        (0x62, 0x6A, 0.0615081787109375),
        (0x62, 0x6F, 0.030755043029785156),
        (0x62, 0x71, 0.030755043029785156),
        (0x62, 0x76, -0.030755043029785156),
        (0x62, 0x77, -0.030755043029785156),
        (0x62, 0x78, -0.030755043029785156),
        (0x62, 0x79, -0.030755043029785156),
        (0x63, 0x68, -0.030755043029785156),
        (0x63, 0x6B, -0.030755043029785156),
        (0x66, 0x21, 0.07986259460449219),
        (0x66, 0x27, 0.07986259460449219),
        (0x66, 0x29, 0.07986259460449219),
        (0x66, 0x3F, 0.07986259460449219),
        (0x66, 0x5D, 0.07986259460449219),
        (0x67, 0x6A, 0.030755043029785156),
        (0x68, 0x62, -0.030755043029785156),
        (0x68, 0x74, -0.030755043029785156),
        (0x68, 0x75, -0.030755043029785156),
        (0x68, 0x76, -0.030755043029785156),
        (0x68, 0x77, -0.030755043029785156),
        (0x68, 0x79, -0.030755043029785156),
        (0x6B, 0x61, -0.0615081787109375),
        (0x6B, 0x63, -0.030755043029785156),
        (0x6B, 0x65, -0.030755043029785156),
        (0x6B, 0x6F, -0.030755043029785156),
        (0x6D, 0x62, -0.030755043029785156),
        (0x6D, 0x74, -0.030755043029785156),
        (0x6D, 0x75, -0.030755043029785156),
        (0x6D, 0x76, -0.030755043029785156),
        (0x6D, 0x77, -0.030755043029785156),
        (0x6D, 0x79, -0.030755043029785156),
        (0x6E, 0x62, -0.030755043029785156),
        (0x6E, 0x74, -0.030755043029785156),
        (0x6E, 0x75, -0.030755043029785156),
        (0x6E, 0x76, -0.030755043029785156),
        (0x6E, 0x77, -0.030755043029785156),
        (0x6E, 0x79, -0.030755043029785156),
        (0x6F, 0x63, 0.030755043029785156),
        (0x6F, 0x64, 0.030755043029785156),
        (0x6F, 0x65, 0.030755043029785156),
        (0x6F, 0x6A, 0.0615081787109375),
        (0x6F, 0x6F, 0.030755043029785156),
        (0x6F, 0x71, 0.030755043029785156),
        (0x6F, 0x76, -0.030755043029785156),
        (0x6F, 0x77, -0.030755043029785156),
        (0x6F, 0x78, -0.030755043029785156),
        (0x6F, 0x79, -0.030755043029785156),
        (0x70, 0x63, 0.030755043029785156),
        (0x70, 0x64, 0.030755043029785156),
        (0x70, 0x65, 0.030755043029785156),
        (0x70, 0x6A, 0.0615081787109375),
        (0x70, 0x6F, 0.030755043029785156),
        (0x70, 0x71, 0.030755043029785156),
        (0x70, 0x76, -0.030755043029785156),
        (0x70, 0x77, -0.030755043029785156),
        (0x70, 0x78, -0.030755043029785156),
        (0x70, 0x79, -0.030755043029785156),
        (0x74, 0x77, -0.030755043029785156),
        (0x74, 0x79, -0.030755043029785156),
        (0x75, 0x77, -0.030755043029785156),
        (0x76, 0x61, -0.0615081787109375),
        (0x76, 0x63, -0.030755043029785156),
        (0x76, 0x65, -0.030755043029785156),
        (0x76, 0x6F, -0.030755043029785156),
        (0x77, 0x61, -0.030755043029785156),
        (0x77, 0x63, -0.030755043029785156),
        (0x77, 0x65, -0.030755043029785156),
        (0x77, 0x6F, -0.030755043029785156),
        (0x79, 0x2C, -0.09226322174072266),
        (0x79, 0x2E, -0.09226322174072266),
        (0x79, 0x61, -0.030755043029785156),
        (0x79, 0x65, -0.030755043029785156),
        (0x79, 0x6F, -0.030755043029785156),
    ],
};

/// `cmr5.tfm`.
#[rustfmt::skip]
pub static CMR5: TfmFont = TfmFont {
    name: "cmr5",
    space: 0.47222900390625,
    skew_char: None,
    params: &[0.0, 0.47222900390625, 0.20833778381347656, 0.13889122009277344, 0.4305543899536133, 1.3611326217651367, 0.13889122009277344],
    chars: &[
        Some(TfmChar { width: 0.8305692672729492, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 1.0972414016723633, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 1.0277957916259766, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.9111270904541016, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.8889036178588867, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.9805727005004883, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.9583492279052734, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 1.0277957916259766, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.9583492279052734, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 1.0277957916259766, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.9583492279052734, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.4027833938598633, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.4027833938598633, height: 0.5, depth: 0.19444561004638672, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.645843505859375, height: 0.5, depth: 0.19444561004638672, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.4027833938598633, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.43750572204589844, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.6805667877197266, height: 0.6284732818603516, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6805667877197266, height: 0.5805540084838867, depth: 0.0, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.9805727005004883, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.6111202239990234, height: 0.0, depth: 0.17014122009277344, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.9583492279052734, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 1.0277957916259766, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.6805667877197266, height: 0.5277767181396484, depth: 0.09722328186035156, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.1777982711791992, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.316690444946289, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 1.0277957916259766, height: 0.7319459915161133, depth: 0.04861164093017578, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.4027833938598633, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.4027833938598633, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 1.0972414016723633, height: 0.6944456100463867, depth: 0.19444561004638672, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.6805667877197266, height: 0.75, depth: 0.05555438995361328, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.0972414016723633, height: 0.75, depth: 0.05555438995361328, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.0277957916259766, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.4027833938598633, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.5416746139526367, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.5416746139526367, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.6805667877197266, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.0277957916259766, height: 0.6666746139526367, depth: 0.16667461395263672, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.4027833938598633, height: 0.122222900390625, depth: 0.19444561004638672, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.47222900390625, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.4027833938598633, height: 0.122222900390625, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.6805667877197266, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.6805667877197266, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.4027833938598633, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.4027833938598633, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 1.0277957916259766, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 1.0277957916259766, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 1.0277957916259766, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.645843505859375, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 1.0277957916259766, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.9805727005004883, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.9347381591796875, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.9583492279052734, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 1.0041837692260742, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.9000148773193359, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.8652925491333008, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 1.0333528518676758, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.9805727005004883, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.49445152282714844, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.6916780471801758, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 1.0152950286865234, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.8305692672729492, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.1889095306396484, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.9805727005004883, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 1.0277957916259766, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.9000148773193359, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 1.0277957916259766, height: 0.6797609329223633, depth: 0.19444561004638672, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.9694614410400391, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.7500123977661133, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.9583492279052734, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.9805727005004883, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.9805727005004883, height: 0.6797609329223633, depth: 0.0, italic: 0.01736164093017578 }), // 0x56
        Some(TfmChar { width: 1.3278017044067383, height: 0.6797609329223633, depth: 0.0, italic: 0.01736164093017578 }), // 0x57
        Some(TfmChar { width: 0.9805727005004883, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.9805727005004883, height: 0.6797609329223633, depth: 0.0, italic: 0.03125 }), // 0x59
        Some(TfmChar { width: 0.8194580078125, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.4027833938598633, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.4027833938598633, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.4027833938598633, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.4027833938598633, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.6805667877197266, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.7500123977661133, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.6111202239990234, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.7500123977661133, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.6111202239990234, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.43750572204589844, height: 0.6944456100463867, depth: 0.0, italic: 0.08819293975830078 }), // 0x66
        Some(TfmChar { width: 0.6805667877197266, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.01736164093017578 }), // 0x67
        Some(TfmChar { width: 0.7500123977661133, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.4027833938598633, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.43750572204589844, height: 0.6797609329223633, depth: 0.19444561004638672, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.7152891159057617, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.4027833938598633, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 1.0972414016723633, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.7500123977661133, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.6805667877197266, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.7500123977661133, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.7152891159057617, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.5416746139526367, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.5486202239990234, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.5416746139526367, height: 0.6150789260864258, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.7500123977661133, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.7152891159057617, height: 0.4305543899536133, depth: 0.0, italic: 0.01736164093017578 }), // 0x76
        Some(TfmChar { width: 0.9583492279052734, height: 0.4305543899536133, depth: 0.0, italic: 0.01736164093017578 }), // 0x77
        Some(TfmChar { width: 0.7152891159057617, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.7152891159057617, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.01736164093017578 }), // 0x79
        Some(TfmChar { width: 0.6111202239990234, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.6805667877197266, height: 0.4305543899536133, depth: 0.0, italic: 0.03472328186035156 }), // 0x7B
        Some(TfmChar { width: 1.3611326217651367, height: 0.4305543899536133, depth: 0.0, italic: 0.03472328186035156 }), // 0x7C
        Some(TfmChar { width: 0.6805667877197266, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.6805667877197266, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.6805667877197266, height: 0.6797609329223633, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x20, 0x4C, -0.44861793518066406),
        (0x20, 0x6C, -0.4027833938598633),
        (0x27, 0x21, 0.13889122009277344),
        (0x27, 0x3F, 0.13889122009277344),
        (0x41, 0x43, -0.03472328186035156),
        (0x41, 0x47, -0.03472328186035156),
        (0x41, 0x4F, -0.03472328186035156),
        (0x41, 0x51, -0.03472328186035156),
        (0x41, 0x54, -0.10416889190673828),
        (0x41, 0x55, -0.03472328186035156),
        (0x41, 0x56, -0.13889122009277344),
        (0x41, 0x57, -0.13889122009277344),
        (0x41, 0x59, -0.10416889190673828),
        (0x41, 0x74, -0.03472328186035156),
        (0x44, 0x41, -0.03472328186035156),
        (0x44, 0x56, -0.03472328186035156),
        (0x44, 0x57, -0.03472328186035156),
        (0x44, 0x58, -0.03472328186035156),
        (0x44, 0x59, -0.03472328186035156),
        (0x46, 0x41, -0.13889122009277344),
        (0x46, 0x43, -0.03472328186035156),
        (0x46, 0x47, -0.03472328186035156),
        (0x46, 0x4F, -0.03472328186035156),
        (0x46, 0x51, -0.03472328186035156),
        (0x46, 0x61, -0.10416889190673828),
        (0x46, 0x65, -0.10416889190673828),
        (0x46, 0x6F, -0.10416889190673828),
        (0x46, 0x72, -0.10416889190673828),
        (0x46, 0x75, -0.10416889190673828),
        (0x49, 0x49, 0.03472328186035156),
        (0x4B, 0x43, -0.03472328186035156),
        (0x4B, 0x47, -0.03472328186035156),
        (0x4B, 0x4F, -0.03472328186035156),
        (0x4B, 0x51, -0.03472328186035156),
        (0x4C, 0x54, -0.10416889190673828),
        (0x4C, 0x56, -0.13889122009277344),
        (0x4C, 0x57, -0.13889122009277344),
        (0x4C, 0x59, -0.10416889190673828),
        (0x4F, 0x41, -0.03472328186035156),
        (0x4F, 0x56, -0.03472328186035156),
        (0x4F, 0x57, -0.03472328186035156),
        (0x4F, 0x58, -0.03472328186035156),
        (0x4F, 0x59, -0.03472328186035156),
        (0x50, 0x2C, -0.10416889190673828),
        (0x50, 0x2E, -0.10416889190673828),
        (0x50, 0x41, -0.10416889190673828),
        (0x50, 0x61, -0.03472328186035156),
        (0x50, 0x65, -0.03472328186035156),
        (0x50, 0x6F, -0.03472328186035156),
        (0x52, 0x43, -0.03472328186035156),
        (0x52, 0x47, -0.03472328186035156),
        (0x52, 0x4F, -0.03472328186035156),
        (0x52, 0x51, -0.03472328186035156),
        (0x52, 0x54, -0.10416889190673828),
        (0x52, 0x55, -0.03472328186035156),
        (0x52, 0x56, -0.13889122009277344),
        (0x52, 0x57, -0.13889122009277344),
        (0x52, 0x59, -0.10416889190673828),
        (0x52, 0x74, -0.03472328186035156),
        (0x54, 0x41, -0.10416889190673828),
        (0x54, 0x61, -0.10416889190673828),
        (0x54, 0x65, -0.10416889190673828),
        (0x54, 0x6F, -0.10416889190673828),
        (0x54, 0x72, -0.10416889190673828),
        (0x54, 0x75, -0.10416889190673828),
        (0x54, 0x79, -0.03472328186035156),
        (0x56, 0x41, -0.13889122009277344),
        (0x56, 0x43, -0.03472328186035156),
        (0x56, 0x47, -0.03472328186035156),
        (0x56, 0x4F, -0.03472328186035156),
        (0x56, 0x51, -0.03472328186035156),
        (0x56, 0x61, -0.10416889190673828),
        (0x56, 0x65, -0.10416889190673828),
        (0x56, 0x6F, -0.10416889190673828),
        (0x56, 0x72, -0.10416889190673828),
        (0x56, 0x75, -0.10416889190673828),
        (0x57, 0x41, -0.13889122009277344),
        (0x57, 0x43, -0.03472328186035156),
        (0x57, 0x47, -0.03472328186035156),
        (0x57, 0x4F, -0.03472328186035156),
        (0x57, 0x51, -0.03472328186035156),
        (0x57, 0x61, -0.10416889190673828),
        (0x57, 0x65, -0.10416889190673828),
        (0x57, 0x6F, -0.10416889190673828),
        (0x57, 0x72, -0.10416889190673828),
        (0x57, 0x75, -0.10416889190673828),
        (0x58, 0x43, -0.03472328186035156),
        (0x58, 0x47, -0.03472328186035156),
        (0x58, 0x4F, -0.03472328186035156),
        (0x58, 0x51, -0.03472328186035156),
        (0x59, 0x41, -0.10416889190673828),
        (0x59, 0x61, -0.10416889190673828),
        (0x59, 0x65, -0.10416889190673828),
        (0x59, 0x6F, -0.10416889190673828),
        (0x59, 0x72, -0.10416889190673828),
        (0x59, 0x75, -0.10416889190673828),
        (0x61, 0x6A, 0.06944561004638672),
        (0x61, 0x76, -0.03472328186035156),
        (0x61, 0x77, -0.03472328186035156),
        (0x61, 0x79, -0.03472328186035156),
        (0x62, 0x63, 0.03472328186035156),
        (0x62, 0x64, 0.03472328186035156),
        (0x62, 0x65, 0.03472328186035156),
        (0x62, 0x6A, 0.06944561004638672),
        (0x62, 0x6F, 0.03472328186035156),
        (0x62, 0x71, 0.03472328186035156),
        (0x62, 0x76, -0.03472328186035156),
        (0x62, 0x77, -0.03472328186035156),
        (0x62, 0x78, -0.03472328186035156),
        (0x62, 0x79, -0.03472328186035156),
        (0x63, 0x68, -0.03472328186035156),
        (0x63, 0x6B, -0.03472328186035156),
        (0x67, 0x6A, 0.03472328186035156),
        (0x68, 0x62, -0.03472328186035156),
        (0x68, 0x74, -0.03472328186035156),
        (0x68, 0x75, -0.03472328186035156),
        (0x68, 0x76, -0.03472328186035156),
        (0x68, 0x77, -0.03472328186035156),
        (0x68, 0x79, -0.03472328186035156),
        (0x6B, 0x61, -0.06944561004638672),
        (0x6B, 0x63, -0.03472328186035156),
        (0x6B, 0x65, -0.03472328186035156),
        (0x6B, 0x6F, -0.03472328186035156),
        (0x6D, 0x62, -0.03472328186035156),
        (0x6D, 0x74, -0.03472328186035156),
        (0x6D, 0x75, -0.03472328186035156),
        (0x6D, 0x76, -0.03472328186035156),
        (0x6D, 0x77, -0.03472328186035156),
        (0x6D, 0x79, -0.03472328186035156),
        (0x6E, 0x62, -0.03472328186035156),
        (0x6E, 0x74, -0.03472328186035156),
        (0x6E, 0x75, -0.03472328186035156),
        (0x6E, 0x76, -0.03472328186035156),
        (0x6E, 0x77, -0.03472328186035156),
        (0x6E, 0x79, -0.03472328186035156),
        (0x6F, 0x63, 0.03472328186035156),
        (0x6F, 0x64, 0.03472328186035156),
        (0x6F, 0x65, 0.03472328186035156),
        (0x6F, 0x6A, 0.06944561004638672),
        (0x6F, 0x6F, 0.03472328186035156),
        (0x6F, 0x71, 0.03472328186035156),
        (0x6F, 0x76, -0.03472328186035156),
        (0x6F, 0x77, -0.03472328186035156),
        (0x6F, 0x78, -0.03472328186035156),
        (0x6F, 0x79, -0.03472328186035156),
        (0x70, 0x63, 0.03472328186035156),
        (0x70, 0x64, 0.03472328186035156),
        (0x70, 0x65, 0.03472328186035156),
        (0x70, 0x6A, 0.06944561004638672),
        (0x70, 0x6F, 0.03472328186035156),
        (0x70, 0x71, 0.03472328186035156),
        (0x70, 0x76, -0.03472328186035156),
        (0x70, 0x77, -0.03472328186035156),
        (0x70, 0x78, -0.03472328186035156),
        (0x70, 0x79, -0.03472328186035156),
        (0x74, 0x77, -0.03472328186035156),
        (0x74, 0x79, -0.03472328186035156),
        (0x75, 0x77, -0.03472328186035156),
        (0x76, 0x61, -0.06944561004638672),
        (0x76, 0x63, -0.03472328186035156),
        (0x76, 0x65, -0.03472328186035156),
        (0x76, 0x6F, -0.03472328186035156),
        (0x77, 0x61, -0.03472328186035156),
        (0x77, 0x63, -0.03472328186035156),
        (0x77, 0x65, -0.03472328186035156),
        (0x77, 0x6F, -0.03472328186035156),
        (0x79, 0x2C, -0.10416889190673828),
        (0x79, 0x2E, -0.10416889190673828),
        (0x79, 0x61, -0.03472328186035156),
        (0x79, 0x65, -0.03472328186035156),
        (0x79, 0x6F, -0.03472328186035156),
    ],
};

/// `cmr6.tfm`.
#[rustfmt::skip]
pub static CMR6: TfmFont = TfmFont {
    name: "cmr6",
    space: 0.41666126251220703,
    skew_char: None,
    params: &[0.0, 0.41666126251220703, 0.1944427490234375, 0.12962818145751953, 0.4305543899536133, 1.2222089767456055, 0.12962818145751953],
    chars: &[
        Some(TfmChar { width: 0.7536954879760742, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.9999895095825195, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.935175895690918, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.8314714431762695, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.805546760559082, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.8962860107421875, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.870361328125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.935175895690918, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.870361328125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.935175895690918, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.870361328125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.7361021041870117, height: 0.6944456100463867, depth: 0.0, italic: 0.08101654052734375 }), // 0x0B
        Some(TfmChar { width: 0.7036943435668945, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.7036943435668945, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 1.0555419921875, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 1.0555419921875, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.35184764862060547, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.38425445556640625, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.6111040115356445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.6111040115356445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.6111040115356445, height: 0.628474235534668, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.6111040115356445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6111040115356445, height: 0.5787019729614258, depth: 0.0, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.8962860107421875, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.546290397644043, height: 0.0, depth: 0.17014026641845703, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.6111040115356445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.870361328125, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.935175895690918, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.6111040115356445, height: 0.5277786254882812, depth: 0.09722423553466797, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.0777664184570312, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.2073945999145508, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.935175895690918, height: 0.7319459915161133, depth: 0.04861164093017578, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.35184764862060547, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.35184764862060547, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.6111040115356445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.9999895095825195, height: 0.6944456100463867, depth: 0.1944427490234375, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.6111040115356445, height: 0.75, depth: 0.05555438995361328, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.9999895095825195, height: 0.75, depth: 0.05555438995361328, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.935175895690918, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.35184764862060547, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.481475830078125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.481475830078125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.6111040115356445, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.935175895690918, height: 0.638885498046875, depth: 0.138885498046875, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.35184764862060547, height: 0.12037181854248047, depth: 0.19444561004638672, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.41666126251220703, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.35184764862060547, height: 0.12037181854248047, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.6111040115356445, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.6111040115356445, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.35184764862060547, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.35184764862060547, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.35184764862060547, height: 0.5, depth: 0.19444561004638672, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.935175895690918, height: 0.38993072509765625, depth: -0.11006927490234375, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.5786972045898438, height: 0.5, depth: 0.19444561004638672, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.5786972045898438, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.935175895690918, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.8962860107421875, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.850916862487793, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.870361328125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.9157304763793945, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.8185091018676758, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.786102294921875, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.9416580200195312, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.8962860107421875, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.44258594512939453, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.6240663528442383, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.9286928176879883, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.7536954879760742, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.090728759765625, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.8962860107421875, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.935175895690918, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.8185091018676758, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.935175895690918, height: 0.6833343505859375, depth: 0.19444561004638672, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.8833236694335938, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.6759185791015625, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.870361328125, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.8962860107421875, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.8962860107421875, height: 0.6833343505859375, depth: 0.0, italic: 0.016204833984375 }), // 0x56
        Some(TfmChar { width: 1.2203569412231445, height: 0.6833343505859375, depth: 0.0, italic: 0.016204833984375 }), // 0x57
        Some(TfmChar { width: 0.8962860107421875, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.8962860107421875, height: 0.6833343505859375, depth: 0.0, italic: 0.02916717529296875 }), // 0x59
        Some(TfmChar { width: 0.7407331466674805, height: 0.6833343505859375, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.35184764862060547, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.6111040115356445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.35184764862060547, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.6111040115356445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.35184764862060547, height: 0.6752653121948242, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.35184764862060547, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.6111040115356445, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.6759185791015625, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.546290397644043, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.6759185791015625, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.5462923049926758, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.38425445556640625, height: 0.6944456100463867, depth: 0.0, italic: 0.08101654052734375 }), // 0x66
        Some(TfmChar { width: 0.6111040115356445, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.016204833984375 }), // 0x67
        Some(TfmChar { width: 0.6759185791015625, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.35184764862060547, height: 0.6752653121948242, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.38425445556640625, height: 0.6752653121948242, depth: 0.19444561004638672, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.6435117721557617, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.35184764862060547, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.9999895095825195, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.6759185791015625, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.6111040115356445, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.6759185791015625, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.6435117721557617, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.481475830078125, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.4879579544067383, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.481475830078125, height: 0.6150789260864258, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.6759185791015625, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.6435117721557617, height: 0.4305543899536133, depth: 0.0, italic: 0.016204833984375 }), // 0x76
        Some(TfmChar { width: 0.870361328125, height: 0.4305543899536133, depth: 0.0, italic: 0.016204833984375 }), // 0x77
        Some(TfmChar { width: 0.6435117721557617, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.6435117721557617, height: 0.4305543899536133, depth: 0.19444561004638672, italic: 0.016204833984375 }), // 0x79
        Some(TfmChar { width: 0.546290397644043, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.6111040115356445, height: 0.4305543899536133, depth: 0.0, italic: 0.03240680694580078 }), // 0x7B
        Some(TfmChar { width: 1.2222089767456055, height: 0.4305543899536133, depth: 0.0, italic: 0.03240680694580078 }), // 0x7C
        Some(TfmChar { width: 0.6111040115356445, height: 0.6944456100463867, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.6111040115356445, height: 0.6752653121948242, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.6111040115356445, height: 0.6752653121948242, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x0B, 0x21, 0.08101654052734375),
        (0x0B, 0x27, 0.08101654052734375),
        (0x0B, 0x29, 0.08101654052734375),
        (0x0B, 0x3F, 0.08101654052734375),
        (0x0B, 0x5D, 0.08101654052734375),
        (0x20, 0x4C, -0.397216796875),
        (0x20, 0x6C, -0.35184764862060547),
        (0x27, 0x21, 0.12962818145751953),
        (0x27, 0x3F, 0.12962818145751953),
        (0x41, 0x43, -0.03240680694580078),
        (0x41, 0x47, -0.03240680694580078),
        (0x41, 0x4F, -0.03240680694580078),
        (0x41, 0x51, -0.03240680694580078),
        (0x41, 0x54, -0.09722137451171875),
        (0x41, 0x55, -0.03240680694580078),
        (0x41, 0x56, -0.12962818145751953),
        (0x41, 0x57, -0.12962818145751953),
        (0x41, 0x59, -0.09722137451171875),
        (0x41, 0x74, -0.03240680694580078),
        (0x44, 0x41, -0.03240680694580078),
        (0x44, 0x56, -0.03240680694580078),
        (0x44, 0x57, -0.03240680694580078),
        (0x44, 0x58, -0.03240680694580078),
        (0x44, 0x59, -0.03240680694580078),
        (0x46, 0x41, -0.12962818145751953),
        (0x46, 0x43, -0.03240680694580078),
        (0x46, 0x47, -0.03240680694580078),
        (0x46, 0x4F, -0.03240680694580078),
        (0x46, 0x51, -0.03240680694580078),
        (0x46, 0x61, -0.09722137451171875),
        (0x46, 0x65, -0.09722137451171875),
        (0x46, 0x6F, -0.09722137451171875),
        (0x46, 0x72, -0.09722137451171875),
        (0x46, 0x75, -0.09722137451171875),
        (0x49, 0x49, 0.03240680694580078),
        (0x4B, 0x43, -0.03240680694580078),
        (0x4B, 0x47, -0.03240680694580078),
        (0x4B, 0x4F, -0.03240680694580078),
        (0x4B, 0x51, -0.03240680694580078),
        (0x4C, 0x54, -0.09722137451171875),
        (0x4C, 0x56, -0.12962818145751953),
        (0x4C, 0x57, -0.12962818145751953),
        (0x4C, 0x59, -0.09722137451171875),
        (0x4F, 0x41, -0.03240680694580078),
        (0x4F, 0x56, -0.03240680694580078),
        (0x4F, 0x57, -0.03240680694580078),
        (0x4F, 0x58, -0.03240680694580078),
        (0x4F, 0x59, -0.03240680694580078),
        (0x50, 0x2C, -0.09722137451171875),
        (0x50, 0x2E, -0.09722137451171875),
        (0x50, 0x41, -0.09722137451171875),
        (0x50, 0x61, -0.03240680694580078),
        (0x50, 0x65, -0.03240680694580078),
        (0x50, 0x6F, -0.03240680694580078),
        (0x52, 0x43, -0.03240680694580078),
        (0x52, 0x47, -0.03240680694580078),
        (0x52, 0x4F, -0.03240680694580078),
        (0x52, 0x51, -0.03240680694580078),
        (0x52, 0x54, -0.09722137451171875),
        (0x52, 0x55, -0.03240680694580078),
        (0x52, 0x56, -0.12962818145751953),
        (0x52, 0x57, -0.12962818145751953),
        (0x52, 0x59, -0.09722137451171875),
        (0x52, 0x74, -0.03240680694580078),
        (0x54, 0x41, -0.09722137451171875),
        (0x54, 0x61, -0.09722137451171875),
        (0x54, 0x65, -0.09722137451171875),
        (0x54, 0x6F, -0.09722137451171875),
        (0x54, 0x72, -0.09722137451171875),
        (0x54, 0x75, -0.09722137451171875),
        (0x54, 0x79, -0.03240680694580078),
        (0x56, 0x41, -0.12962818145751953),
        (0x56, 0x43, -0.03240680694580078),
        (0x56, 0x47, -0.03240680694580078),
        (0x56, 0x4F, -0.03240680694580078),
        (0x56, 0x51, -0.03240680694580078),
        (0x56, 0x61, -0.09722137451171875),
        (0x56, 0x65, -0.09722137451171875),
        (0x56, 0x6F, -0.09722137451171875),
        (0x56, 0x72, -0.09722137451171875),
        (0x56, 0x75, -0.09722137451171875),
        (0x57, 0x41, -0.12962818145751953),
        (0x57, 0x43, -0.03240680694580078),
        (0x57, 0x47, -0.03240680694580078),
        (0x57, 0x4F, -0.03240680694580078),
        (0x57, 0x51, -0.03240680694580078),
        (0x57, 0x61, -0.09722137451171875),
        (0x57, 0x65, -0.09722137451171875),
        (0x57, 0x6F, -0.09722137451171875),
        (0x57, 0x72, -0.09722137451171875),
        (0x57, 0x75, -0.09722137451171875),
        (0x58, 0x43, -0.03240680694580078),
        (0x58, 0x47, -0.03240680694580078),
        (0x58, 0x4F, -0.03240680694580078),
        (0x58, 0x51, -0.03240680694580078),
        (0x59, 0x41, -0.09722137451171875),
        (0x59, 0x61, -0.09722137451171875),
        (0x59, 0x65, -0.09722137451171875),
        (0x59, 0x6F, -0.09722137451171875),
        (0x59, 0x72, -0.09722137451171875),
        (0x59, 0x75, -0.09722137451171875),
        (0x61, 0x6A, 0.06481456756591797),
        (0x61, 0x76, -0.03240680694580078),
        (0x61, 0x77, -0.03240680694580078),
        (0x61, 0x79, -0.03240680694580078),
        (0x62, 0x63, 0.03240680694580078),
        (0x62, 0x64, 0.03240680694580078),
        (0x62, 0x65, 0.03240680694580078),
        (0x62, 0x6A, 0.06481456756591797),
        (0x62, 0x6F, 0.03240680694580078),
        (0x62, 0x71, 0.03240680694580078),
        (0x62, 0x76, -0.03240680694580078),
        (0x62, 0x77, -0.03240680694580078),
        (0x62, 0x78, -0.03240680694580078),
        (0x62, 0x79, -0.03240680694580078),
        (0x63, 0x68, -0.03240680694580078),
        (0x63, 0x6B, -0.03240680694580078),
        (0x66, 0x21, 0.08101654052734375),
        (0x66, 0x27, 0.08101654052734375),
        (0x66, 0x29, 0.08101654052734375),
        (0x66, 0x3F, 0.08101654052734375),
        (0x66, 0x5D, 0.08101654052734375),
        (0x67, 0x6A, 0.03240680694580078),
        (0x68, 0x62, -0.03240680694580078),
        (0x68, 0x74, -0.03240680694580078),
        (0x68, 0x75, -0.03240680694580078),
        (0x68, 0x76, -0.03240680694580078),
        (0x68, 0x77, -0.03240680694580078),
        (0x68, 0x79, -0.03240680694580078),
        (0x6B, 0x61, -0.06481456756591797),
        (0x6B, 0x63, -0.03240680694580078),
        (0x6B, 0x65, -0.03240680694580078),
        (0x6B, 0x6F, -0.03240680694580078),
        (0x6D, 0x62, -0.03240680694580078),
        (0x6D, 0x74, -0.03240680694580078),
        (0x6D, 0x75, -0.03240680694580078),
        (0x6D, 0x76, -0.03240680694580078),
        (0x6D, 0x77, -0.03240680694580078),
        (0x6D, 0x79, -0.03240680694580078),
        (0x6E, 0x62, -0.03240680694580078),
        (0x6E, 0x74, -0.03240680694580078),
        (0x6E, 0x75, -0.03240680694580078),
        (0x6E, 0x76, -0.03240680694580078),
        (0x6E, 0x77, -0.03240680694580078),
        (0x6E, 0x79, -0.03240680694580078),
        (0x6F, 0x63, 0.03240680694580078),
        (0x6F, 0x64, 0.03240680694580078),
        (0x6F, 0x65, 0.03240680694580078),
        (0x6F, 0x6A, 0.06481456756591797),
        (0x6F, 0x6F, 0.03240680694580078),
        (0x6F, 0x71, 0.03240680694580078),
        (0x6F, 0x76, -0.03240680694580078),
        (0x6F, 0x77, -0.03240680694580078),
        (0x6F, 0x78, -0.03240680694580078),
        (0x6F, 0x79, -0.03240680694580078),
        (0x70, 0x63, 0.03240680694580078),
        (0x70, 0x64, 0.03240680694580078),
        (0x70, 0x65, 0.03240680694580078),
        (0x70, 0x6A, 0.06481456756591797),
        (0x70, 0x6F, 0.03240680694580078),
        (0x70, 0x71, 0.03240680694580078),
        (0x70, 0x76, -0.03240680694580078),
        (0x70, 0x77, -0.03240680694580078),
        (0x70, 0x78, -0.03240680694580078),
        (0x70, 0x79, -0.03240680694580078),
        (0x74, 0x77, -0.03240680694580078),
        (0x74, 0x79, -0.03240680694580078),
        (0x75, 0x77, -0.03240680694580078),
        (0x76, 0x61, -0.06481456756591797),
        (0x76, 0x63, -0.03240680694580078),
        (0x76, 0x65, -0.03240680694580078),
        (0x76, 0x6F, -0.03240680694580078),
        (0x77, 0x61, -0.03240680694580078),
        (0x77, 0x63, -0.03240680694580078),
        (0x77, 0x65, -0.03240680694580078),
        (0x77, 0x6F, -0.03240680694580078),
        (0x79, 0x2C, -0.09722137451171875),
        (0x79, 0x2E, -0.09722137451171875),
        (0x79, 0x61, -0.03240680694580078),
        (0x79, 0x65, -0.03240680694580078),
        (0x79, 0x6F, -0.03240680694580078),
    ],
};

/// `cmr8.tfm`.
#[rustfmt::skip]
pub static CMR8: TfmFont = TfmFont {
    name: "cmr8",
    space: 0.3541717529296875,
    skew_char: None,
    params: &[0.0, 0.3541717529296875, 0.17708587646484375, 0.1180572509765625, 0.4305553436279297, 1.0625152587890625, 0.1180572509765625],
    chars: &[
        Some(TfmChar { width: 0.6635513305664062, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.8854293823242188, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.8264007568359375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.73681640625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.708343505859375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.7673721313476562, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.8264007568359375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.7673721313476562, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.8264007568359375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.7673721313476562, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.6198005676269531, height: 0.6944446563720703, depth: 0.0, italic: 0.07899284362792969 }), // 0x0B
        Some(TfmChar { width: 0.5902862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.5902862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.8854293823242188, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.8854293823242188, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.29514312744140625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.3246574401855469, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.5312576293945312, height: 0.6284732818603516, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.5312576293945312, height: 0.5736103057861328, depth: 0.0, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.7958450317382812, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.47222900390625, height: 0.0, depth: 0.17013931274414062, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.7673721313476562, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.8264007568359375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.5312576293945312, height: 0.5277786254882812, depth: 0.09722328186035156, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.9586944580078125, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.076751708984375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.8264007568359375, height: 0.7319431304931641, depth: 0.048610687255859375, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.29514312744140625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.29514312744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.8854293823242188, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.8854293823242188, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.8264007568359375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.29514312744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.41320037841796875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.41320037841796875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.8264007568359375, height: 0.6041717529296875, depth: 0.1041717529296875, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.29514312744140625, height: 0.11111068725585938, depth: 0.1944446563720703, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.3541717529296875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.29514312744140625, height: 0.11111068725585938, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.5312576293945312, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5312576293945312, height: 0.6444454193115234, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.29514312744140625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.29514312744140625, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.29514312744140625, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.8264007568359375, height: 0.3779869079589844, depth: -0.12201309204101562, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.5017433166503906, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.5017433166503906, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.8264007568359375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.7520942687988281, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.7673721313476562, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.8111228942871094, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.7225799560546875, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6930656433105469, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.8335189819335938, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.3826446533203125, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.5454940795898438, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.8253593444824219, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.6635513305664062, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.972930908203125, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.8264007568359375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.7225799560546875, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.8264007568359375, height: 0.6833324432373047, depth: 0.1944446563720703, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7816085815429688, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.5902862548828125, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.7673721313476562, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.014757156372070312 }), // 0x56
        Some(TfmChar { width: 1.0909881591796875, height: 0.6833324432373047, depth: 0.0, italic: 0.014757156372070312 }), // 0x57
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.7958450317382812, height: 0.6833324432373047, depth: 0.0, italic: 0.026563644409179688 }), // 0x59
        Some(TfmChar { width: 0.6493148803710938, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.29514312744140625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.29514312744140625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.29514312744140625, height: 0.6706352233886719, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.29514312744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.5902862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.47222900390625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.5902862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.47222900390625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.3246574401855469, height: 0.6944446563720703, depth: 0.0, italic: 0.07899284362792969 }), // 0x66
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.014757156372070312 }), // 0x67
        Some(TfmChar { width: 0.5902862548828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.29514312744140625, height: 0.6706352233886719, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.3246574401855469, height: 0.6706352233886719, depth: 0.1944446563720703, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5607719421386719, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.29514312744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.8854293823242188, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.5902862548828125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5902862548828125, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.5607719421386719, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.41406822204589844, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.41910362243652344, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.41320037841796875, height: 0.6150798797607422, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.5902862548828125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.5607719421386719, height: 0.4305553436279297, depth: 0.0, italic: 0.014757156372070312 }), // 0x76
        Some(TfmChar { width: 0.7673721313476562, height: 0.4305553436279297, depth: 0.0, italic: 0.014757156372070312 }), // 0x77
        Some(TfmChar { width: 0.5607719421386719, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5607719421386719, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.014757156372070312 }), // 0x79
        Some(TfmChar { width: 0.47222900390625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.5312576293945312, height: 0.4305553436279297, depth: 0.0, italic: 0.029514312744140625 }), // 0x7B
        Some(TfmChar { width: 1.0625152587890625, height: 0.4305553436279297, depth: 0.0, italic: 0.029514312744140625 }), // 0x7C
        Some(TfmChar { width: 0.5312576293945312, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.5312576293945312, height: 0.6706352233886719, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.5312576293945312, height: 0.6706352233886719, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x0B, 0x21, 0.07899284362792969),
        (0x0B, 0x27, 0.07899284362792969),
        (0x0B, 0x29, 0.07899284362792969),
        (0x0B, 0x3F, 0.07899284362792969),
        (0x0B, 0x5D, 0.07899284362792969),
        (0x20, 0x4C, -0.3388938903808594),
        (0x20, 0x6C, -0.29514312744140625),
        (0x27, 0x21, 0.1180572509765625),
        (0x27, 0x3F, 0.1180572509765625),
        (0x41, 0x43, -0.029514312744140625),
        (0x41, 0x47, -0.029514312744140625),
        (0x41, 0x4F, -0.029514312744140625),
        (0x41, 0x51, -0.029514312744140625),
        (0x41, 0x54, -0.08854293823242188),
        (0x41, 0x55, -0.029514312744140625),
        (0x41, 0x56, -0.1180572509765625),
        (0x41, 0x57, -0.1180572509765625),
        (0x41, 0x59, -0.08854293823242188),
        (0x41, 0x74, -0.029514312744140625),
        (0x44, 0x41, -0.029514312744140625),
        (0x44, 0x56, -0.029514312744140625),
        (0x44, 0x57, -0.029514312744140625),
        (0x44, 0x58, -0.029514312744140625),
        (0x44, 0x59, -0.029514312744140625),
        (0x46, 0x41, -0.1180572509765625),
        (0x46, 0x43, -0.029514312744140625),
        (0x46, 0x47, -0.029514312744140625),
        (0x46, 0x4F, -0.029514312744140625),
        (0x46, 0x51, -0.029514312744140625),
        (0x46, 0x61, -0.08854293823242188),
        (0x46, 0x65, -0.08854293823242188),
        (0x46, 0x6F, -0.08854293823242188),
        (0x46, 0x72, -0.08854293823242188),
        (0x46, 0x75, -0.08854293823242188),
        (0x49, 0x49, 0.029514312744140625),
        (0x4B, 0x43, -0.029514312744140625),
        (0x4B, 0x47, -0.029514312744140625),
        (0x4B, 0x4F, -0.029514312744140625),
        (0x4B, 0x51, -0.029514312744140625),
        (0x4C, 0x54, -0.08854293823242188),
        (0x4C, 0x56, -0.1180572509765625),
        (0x4C, 0x57, -0.1180572509765625),
        (0x4C, 0x59, -0.08854293823242188),
        (0x4F, 0x41, -0.029514312744140625),
        (0x4F, 0x56, -0.029514312744140625),
        (0x4F, 0x57, -0.029514312744140625),
        (0x4F, 0x58, -0.029514312744140625),
        (0x4F, 0x59, -0.029514312744140625),
        (0x50, 0x2C, -0.08854293823242188),
        (0x50, 0x2E, -0.08854293823242188),
        (0x50, 0x41, -0.08854293823242188),
        (0x50, 0x61, -0.029514312744140625),
        (0x50, 0x65, -0.029514312744140625),
        (0x50, 0x6F, -0.029514312744140625),
        (0x52, 0x43, -0.029514312744140625),
        (0x52, 0x47, -0.029514312744140625),
        (0x52, 0x4F, -0.029514312744140625),
        (0x52, 0x51, -0.029514312744140625),
        (0x52, 0x54, -0.08854293823242188),
        (0x52, 0x55, -0.029514312744140625),
        (0x52, 0x56, -0.1180572509765625),
        (0x52, 0x57, -0.1180572509765625),
        (0x52, 0x59, -0.08854293823242188),
        (0x52, 0x74, -0.029514312744140625),
        (0x54, 0x41, -0.08854293823242188),
        (0x54, 0x61, -0.08854293823242188),
        (0x54, 0x65, -0.08854293823242188),
        (0x54, 0x6F, -0.08854293823242188),
        (0x54, 0x72, -0.08854293823242188),
        (0x54, 0x75, -0.08854293823242188),
        (0x54, 0x79, -0.029514312744140625),
        (0x56, 0x41, -0.1180572509765625),
        (0x56, 0x43, -0.029514312744140625),
        (0x56, 0x47, -0.029514312744140625),
        (0x56, 0x4F, -0.029514312744140625),
        (0x56, 0x51, -0.029514312744140625),
        (0x56, 0x61, -0.08854293823242188),
        (0x56, 0x65, -0.08854293823242188),
        (0x56, 0x6F, -0.08854293823242188),
        (0x56, 0x72, -0.08854293823242188),
        (0x56, 0x75, -0.08854293823242188),
        (0x57, 0x41, -0.1180572509765625),
        (0x57, 0x43, -0.029514312744140625),
        (0x57, 0x47, -0.029514312744140625),
        (0x57, 0x4F, -0.029514312744140625),
        (0x57, 0x51, -0.029514312744140625),
        (0x57, 0x61, -0.08854293823242188),
        (0x57, 0x65, -0.08854293823242188),
        (0x57, 0x6F, -0.08854293823242188),
        (0x57, 0x72, -0.08854293823242188),
        (0x57, 0x75, -0.08854293823242188),
        (0x58, 0x43, -0.029514312744140625),
        (0x58, 0x47, -0.029514312744140625),
        (0x58, 0x4F, -0.029514312744140625),
        (0x58, 0x51, -0.029514312744140625),
        (0x59, 0x41, -0.08854293823242188),
        (0x59, 0x61, -0.08854293823242188),
        (0x59, 0x65, -0.08854293823242188),
        (0x59, 0x6F, -0.08854293823242188),
        (0x59, 0x72, -0.08854293823242188),
        (0x59, 0x75, -0.08854293823242188),
        (0x61, 0x6A, 0.05902862548828125),
        (0x61, 0x76, -0.029514312744140625),
        (0x61, 0x77, -0.029514312744140625),
        (0x61, 0x79, -0.029514312744140625),
        (0x62, 0x63, 0.029514312744140625),
        (0x62, 0x64, 0.029514312744140625),
        (0x62, 0x65, 0.029514312744140625),
        (0x62, 0x6A, 0.05902862548828125),
        (0x62, 0x6F, 0.029514312744140625),
        (0x62, 0x71, 0.029514312744140625),
        (0x62, 0x76, -0.029514312744140625),
        (0x62, 0x77, -0.029514312744140625),
        (0x62, 0x78, -0.029514312744140625),
        (0x62, 0x79, -0.029514312744140625),
        (0x63, 0x68, -0.029514312744140625),
        (0x63, 0x6B, -0.029514312744140625),
        (0x66, 0x21, 0.07899284362792969),
        (0x66, 0x27, 0.07899284362792969),
        (0x66, 0x29, 0.07899284362792969),
        (0x66, 0x3F, 0.07899284362792969),
        (0x66, 0x5D, 0.07899284362792969),
        (0x67, 0x6A, 0.029514312744140625),
        (0x68, 0x62, -0.029514312744140625),
        (0x68, 0x74, -0.029514312744140625),
        (0x68, 0x75, -0.029514312744140625),
        (0x68, 0x76, -0.029514312744140625),
        (0x68, 0x77, -0.029514312744140625),
        (0x68, 0x79, -0.029514312744140625),
        (0x6B, 0x61, -0.05902862548828125),
        (0x6B, 0x63, -0.029514312744140625),
        (0x6B, 0x65, -0.029514312744140625),
        (0x6B, 0x6F, -0.029514312744140625),
        (0x6D, 0x62, -0.029514312744140625),
        (0x6D, 0x74, -0.029514312744140625),
        (0x6D, 0x75, -0.029514312744140625),
        (0x6D, 0x76, -0.029514312744140625),
        (0x6D, 0x77, -0.029514312744140625),
        (0x6D, 0x79, -0.029514312744140625),
        (0x6E, 0x62, -0.029514312744140625),
        (0x6E, 0x74, -0.029514312744140625),
        (0x6E, 0x75, -0.029514312744140625),
        (0x6E, 0x76, -0.029514312744140625),
        (0x6E, 0x77, -0.029514312744140625),
        (0x6E, 0x79, -0.029514312744140625),
        (0x6F, 0x63, 0.029514312744140625),
        (0x6F, 0x64, 0.029514312744140625),
        (0x6F, 0x65, 0.029514312744140625),
        (0x6F, 0x6A, 0.05902862548828125),
        (0x6F, 0x6F, 0.029514312744140625),
        (0x6F, 0x71, 0.029514312744140625),
        (0x6F, 0x76, -0.029514312744140625),
        (0x6F, 0x77, -0.029514312744140625),
        (0x6F, 0x78, -0.029514312744140625),
        (0x6F, 0x79, -0.029514312744140625),
        (0x70, 0x63, 0.029514312744140625),
        (0x70, 0x64, 0.029514312744140625),
        (0x70, 0x65, 0.029514312744140625),
        (0x70, 0x6A, 0.05902862548828125),
        (0x70, 0x6F, 0.029514312744140625),
        (0x70, 0x71, 0.029514312744140625),
        (0x70, 0x76, -0.029514312744140625),
        (0x70, 0x77, -0.029514312744140625),
        (0x70, 0x78, -0.029514312744140625),
        (0x70, 0x79, -0.029514312744140625),
        (0x74, 0x77, -0.029514312744140625),
        (0x74, 0x79, -0.029514312744140625),
        (0x75, 0x77, -0.029514312744140625),
        (0x76, 0x61, -0.05902862548828125),
        (0x76, 0x63, -0.029514312744140625),
        (0x76, 0x65, -0.029514312744140625),
        (0x76, 0x6F, -0.029514312744140625),
        (0x77, 0x61, -0.029514312744140625),
        (0x77, 0x63, -0.029514312744140625),
        (0x77, 0x65, -0.029514312744140625),
        (0x77, 0x6F, -0.029514312744140625),
        (0x79, 0x2C, -0.08854293823242188),
        (0x79, 0x2E, -0.08854293823242188),
        (0x79, 0x61, -0.029514312744140625),
        (0x79, 0x65, -0.029514312744140625),
        (0x79, 0x6F, -0.029514312744140625),
    ],
};

/// `cmr9.tfm`.
#[rustfmt::skip]
pub static CMR9: TfmFont = TfmFont {
    name: "cmr9",
    space: 0.34259033203125,
    skew_char: None,
    params: &[0.0, 0.34259033203125, 0.171295166015625, 0.11419677734375, 0.4305553436279297, 1.02777099609375, 0.11419677734375],
    chars: &[
        Some(TfmChar { width: 0.6422796249389648, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.856475830078125, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.79937744140625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.7135753631591797, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.6851806640625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.742279052734375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.79937744140625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.742279052734375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.79937744140625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.742279052734375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.5995330810546875, height: 0.6944446563720703, depth: 0.0, italic: 0.07831668853759766 }), // 0x0B
        Some(TfmChar { width: 0.57098388671875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.57098388671875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.856475830078125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.856475830078125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.285491943359375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.3140411376953125, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.513885498046875, height: 0.6284723281860352, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.513885498046875, height: 0.5706768035888672, depth: 0.0, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.7706737518310547, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.456787109375, height: 0.0, depth: 0.17013931274414062, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.742279052734375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.79937744140625, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.513885498046875, height: 0.5277776718139648, depth: 0.09722232818603516, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.9277715682983398, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.0419683456420898, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.79937744140625, height: 0.7319440841674805, depth: 0.048610687255859375, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.285491943359375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.285491943359375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.856475830078125, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.856475830078125, height: 0.75, depth: 0.05555534362792969, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.79937744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.285491943359375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.399688720703125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.399688720703125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.79937744140625, height: 0.59259033203125, depth: 0.09259033203125, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.285491943359375, height: 0.10802555084228516, depth: 0.1944446563720703, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.34259033203125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.285491943359375, height: 0.10802555084228516, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.513885498046875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.513885498046875, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.285491943359375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.285491943359375, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.285491943359375, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.79937744140625, height: 0.37229251861572266, depth: -0.12770748138427734, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.4853363037109375, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.4853363037109375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.79937744140625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.7279272079467773, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.742279052734375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.7850255966186523, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6993780136108398, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6708288192749023, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.8064765930175781, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.3709850311279297, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.5280828475952148, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.7992229461669922, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.6422796249389648, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.9419689178466797, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.79937744140625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.6993780136108398, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.79937744140625, height: 0.6833324432373047, depth: 0.1944446563720703, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7564764022827148, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.57098388671875, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.742279052734375, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.014275550842285156 }), // 0x56
        Some(TfmChar { width: 1.0561656951904297, height: 0.6833324432373047, depth: 0.0, italic: 0.014275550842285156 }), // 0x57
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.7706737518310547, height: 0.6833324432373047, depth: 0.0, italic: 0.025693893432617188 }), // 0x59
        Some(TfmChar { width: 0.628082275390625, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.285491943359375, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.285491943359375, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.285491943359375, height: 0.669093132019043, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.285491943359375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.57098388671875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.456787109375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.57098388671875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.45717334747314453, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.3140411376953125, height: 0.6944446563720703, depth: 0.0, italic: 0.07831668853759766 }), // 0x66
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.014275550842285156 }), // 0x67
        Some(TfmChar { width: 0.57098388671875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.285491943359375, height: 0.669093132019043, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.3140411376953125, height: 0.669093132019043, depth: 0.1944446563720703, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5424346923828125, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.285491943359375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.856475830078125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.57098388671875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.57098388671875, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.5424346923828125, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.4020042419433594, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.4053993225097656, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.399688720703125, height: 0.6150798797607422, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.57098388671875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.5424346923828125, height: 0.4305553436279297, depth: 0.0, italic: 0.014275550842285156 }), // 0x76
        Some(TfmChar { width: 0.742279052734375, height: 0.4305553436279297, depth: 0.0, italic: 0.014275550842285156 }), // 0x77
        Some(TfmChar { width: 0.5424346923828125, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5424346923828125, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.014275550842285156 }), // 0x79
        Some(TfmChar { width: 0.456787109375, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.513885498046875, height: 0.4305553436279297, depth: 0.0, italic: 0.0285491943359375 }), // 0x7B
        Some(TfmChar { width: 1.02777099609375, height: 0.4305553436279297, depth: 0.0, italic: 0.0285491943359375 }), // 0x7C
        Some(TfmChar { width: 0.513885498046875, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.513885498046875, height: 0.669093132019043, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.513885498046875, height: 0.669093132019043, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x0B, 0x21, 0.07831668853759766),
        (0x0B, 0x27, 0.07831668853759766),
        (0x0B, 0x29, 0.07831668853759766),
        (0x0B, 0x3F, 0.07831668853759766),
        (0x0B, 0x5D, 0.07831668853759766),
        (0x20, 0x4C, -0.32823848724365234),
        (0x20, 0x6C, -0.285491943359375),
        (0x27, 0x21, 0.11419677734375),
        (0x27, 0x3F, 0.11419677734375),
        (0x41, 0x43, -0.0285491943359375),
        (0x41, 0x47, -0.0285491943359375),
        (0x41, 0x4F, -0.0285491943359375),
        (0x41, 0x51, -0.0285491943359375),
        (0x41, 0x54, -0.0856475830078125),
        (0x41, 0x55, -0.0285491943359375),
        (0x41, 0x56, -0.11419677734375),
        (0x41, 0x57, -0.11419677734375),
        (0x41, 0x59, -0.0856475830078125),
        (0x41, 0x74, -0.0285491943359375),
        (0x44, 0x41, -0.0285491943359375),
        (0x44, 0x56, -0.0285491943359375),
        (0x44, 0x57, -0.0285491943359375),
        (0x44, 0x58, -0.0285491943359375),
        (0x44, 0x59, -0.0285491943359375),
        (0x46, 0x41, -0.11419677734375),
        (0x46, 0x43, -0.0285491943359375),
        (0x46, 0x47, -0.0285491943359375),
        (0x46, 0x4F, -0.0285491943359375),
        (0x46, 0x51, -0.0285491943359375),
        (0x46, 0x61, -0.0856475830078125),
        (0x46, 0x65, -0.0856475830078125),
        (0x46, 0x6F, -0.0856475830078125),
        (0x46, 0x72, -0.0856475830078125),
        (0x46, 0x75, -0.0856475830078125),
        (0x49, 0x49, 0.0285491943359375),
        (0x4B, 0x43, -0.0285491943359375),
        (0x4B, 0x47, -0.0285491943359375),
        (0x4B, 0x4F, -0.0285491943359375),
        (0x4B, 0x51, -0.0285491943359375),
        (0x4C, 0x54, -0.0856475830078125),
        (0x4C, 0x56, -0.11419677734375),
        (0x4C, 0x57, -0.11419677734375),
        (0x4C, 0x59, -0.0856475830078125),
        (0x4F, 0x41, -0.0285491943359375),
        (0x4F, 0x56, -0.0285491943359375),
        (0x4F, 0x57, -0.0285491943359375),
        (0x4F, 0x58, -0.0285491943359375),
        (0x4F, 0x59, -0.0285491943359375),
        (0x50, 0x2C, -0.0856475830078125),
        (0x50, 0x2E, -0.0856475830078125),
        (0x50, 0x41, -0.0856475830078125),
        (0x50, 0x61, -0.0285491943359375),
        (0x50, 0x65, -0.0285491943359375),
        (0x50, 0x6F, -0.0285491943359375),
        (0x52, 0x43, -0.0285491943359375),
        (0x52, 0x47, -0.0285491943359375),
        (0x52, 0x4F, -0.0285491943359375),
        (0x52, 0x51, -0.0285491943359375),
        (0x52, 0x54, -0.0856475830078125),
        (0x52, 0x55, -0.0285491943359375),
        (0x52, 0x56, -0.11419677734375),
        (0x52, 0x57, -0.11419677734375),
        (0x52, 0x59, -0.0856475830078125),
        (0x52, 0x74, -0.0285491943359375),
        (0x54, 0x41, -0.0856475830078125),
        (0x54, 0x61, -0.0856475830078125),
        (0x54, 0x65, -0.0856475830078125),
        (0x54, 0x6F, -0.0856475830078125),
        (0x54, 0x72, -0.0856475830078125),
        (0x54, 0x75, -0.0856475830078125),
        (0x54, 0x79, -0.0285491943359375),
        (0x56, 0x41, -0.11419677734375),
        (0x56, 0x43, -0.0285491943359375),
        (0x56, 0x47, -0.0285491943359375),
        (0x56, 0x4F, -0.0285491943359375),
        (0x56, 0x51, -0.0285491943359375),
        (0x56, 0x61, -0.0856475830078125),
        (0x56, 0x65, -0.0856475830078125),
        (0x56, 0x6F, -0.0856475830078125),
        (0x56, 0x72, -0.0856475830078125),
        (0x56, 0x75, -0.0856475830078125),
        (0x57, 0x41, -0.11419677734375),
        (0x57, 0x43, -0.0285491943359375),
        (0x57, 0x47, -0.0285491943359375),
        (0x57, 0x4F, -0.0285491943359375),
        (0x57, 0x51, -0.0285491943359375),
        (0x57, 0x61, -0.0856475830078125),
        (0x57, 0x65, -0.0856475830078125),
        (0x57, 0x6F, -0.0856475830078125),
        (0x57, 0x72, -0.0856475830078125),
        (0x57, 0x75, -0.0856475830078125),
        (0x58, 0x43, -0.0285491943359375),
        (0x58, 0x47, -0.0285491943359375),
        (0x58, 0x4F, -0.0285491943359375),
        (0x58, 0x51, -0.0285491943359375),
        (0x59, 0x41, -0.0856475830078125),
        (0x59, 0x61, -0.0856475830078125),
        (0x59, 0x65, -0.0856475830078125),
        (0x59, 0x6F, -0.0856475830078125),
        (0x59, 0x72, -0.0856475830078125),
        (0x59, 0x75, -0.0856475830078125),
        (0x61, 0x6A, 0.057098388671875),
        (0x61, 0x76, -0.0285491943359375),
        (0x61, 0x77, -0.0285491943359375),
        (0x61, 0x79, -0.0285491943359375),
        (0x62, 0x63, 0.0285491943359375),
        (0x62, 0x64, 0.0285491943359375),
        (0x62, 0x65, 0.0285491943359375),
        (0x62, 0x6A, 0.057098388671875),
        (0x62, 0x6F, 0.0285491943359375),
        (0x62, 0x71, 0.0285491943359375),
        (0x62, 0x76, -0.0285491943359375),
        (0x62, 0x77, -0.0285491943359375),
        (0x62, 0x78, -0.0285491943359375),
        (0x62, 0x79, -0.0285491943359375),
        (0x63, 0x68, -0.0285491943359375),
        (0x63, 0x6B, -0.0285491943359375),
        (0x66, 0x21, 0.07831668853759766),
        (0x66, 0x27, 0.07831668853759766),
        (0x66, 0x29, 0.07831668853759766),
        (0x66, 0x3F, 0.07831668853759766),
        (0x66, 0x5D, 0.07831668853759766),
        (0x67, 0x6A, 0.0285491943359375),
        (0x68, 0x62, -0.0285491943359375),
        (0x68, 0x74, -0.0285491943359375),
        (0x68, 0x75, -0.0285491943359375),
        (0x68, 0x76, -0.0285491943359375),
        (0x68, 0x77, -0.0285491943359375),
        (0x68, 0x79, -0.0285491943359375),
        (0x6B, 0x61, -0.057098388671875),
        (0x6B, 0x63, -0.0285491943359375),
        (0x6B, 0x65, -0.0285491943359375),
        (0x6B, 0x6F, -0.0285491943359375),
        (0x6D, 0x62, -0.0285491943359375),
        (0x6D, 0x74, -0.0285491943359375),
        (0x6D, 0x75, -0.0285491943359375),
        (0x6D, 0x76, -0.0285491943359375),
        (0x6D, 0x77, -0.0285491943359375),
        (0x6D, 0x79, -0.0285491943359375),
        (0x6E, 0x62, -0.0285491943359375),
        (0x6E, 0x74, -0.0285491943359375),
        (0x6E, 0x75, -0.0285491943359375),
        (0x6E, 0x76, -0.0285491943359375),
        (0x6E, 0x77, -0.0285491943359375),
        (0x6E, 0x79, -0.0285491943359375),
        (0x6F, 0x63, 0.0285491943359375),
        (0x6F, 0x64, 0.0285491943359375),
        (0x6F, 0x65, 0.0285491943359375),
        (0x6F, 0x6A, 0.057098388671875),
        (0x6F, 0x6F, 0.0285491943359375),
        (0x6F, 0x71, 0.0285491943359375),
        (0x6F, 0x76, -0.0285491943359375),
        (0x6F, 0x77, -0.0285491943359375),
        (0x6F, 0x78, -0.0285491943359375),
        (0x6F, 0x79, -0.0285491943359375),
        (0x70, 0x63, 0.0285491943359375),
        (0x70, 0x64, 0.0285491943359375),
        (0x70, 0x65, 0.0285491943359375),
        (0x70, 0x6A, 0.057098388671875),
        (0x70, 0x6F, 0.0285491943359375),
        (0x70, 0x71, 0.0285491943359375),
        (0x70, 0x76, -0.0285491943359375),
        (0x70, 0x77, -0.0285491943359375),
        (0x70, 0x78, -0.0285491943359375),
        (0x70, 0x79, -0.0285491943359375),
        (0x74, 0x77, -0.0285491943359375),
        (0x74, 0x79, -0.0285491943359375),
        (0x75, 0x77, -0.0285491943359375),
        (0x76, 0x61, -0.057098388671875),
        (0x76, 0x63, -0.0285491943359375),
        (0x76, 0x65, -0.0285491943359375),
        (0x76, 0x6F, -0.0285491943359375),
        (0x77, 0x61, -0.0285491943359375),
        (0x77, 0x63, -0.0285491943359375),
        (0x77, 0x65, -0.0285491943359375),
        (0x77, 0x6F, -0.0285491943359375),
        (0x79, 0x2C, -0.0856475830078125),
        (0x79, 0x2E, -0.0856475830078125),
        (0x79, 0x61, -0.0285491943359375),
        (0x79, 0x65, -0.0285491943359375),
        (0x79, 0x6F, -0.0285491943359375),
    ],
};

/// `cmr12.tfm`.
#[rustfmt::skip]
pub static CMR12: TfmFont = TfmFont {
    name: "cmr12",
    space: 0.326385498046875,
    skew_char: None,
    params: &[0.0, 0.326385498046875, 0.1631927490234375, 0.108795166015625, 0.4305562973022461, 0.979156494140625, 0.108795166015625],
    chars: &[
        Some(TfmChar { width: 0.6118001937866211, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.8159637451171875, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.761566162109375, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.6796236038208008, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.65277099609375, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.7071685791015625, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.761566162109375, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.7071685791015625, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.761566162109375, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.7071685791015625, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.5711746215820312, height: 0.6944437026977539, depth: 0.0, italic: 0.0697336196899414 }), // 0x0B
        Some(TfmChar { width: 0.543975830078125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.543975830078125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.8159637451171875, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.8159637451171875, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.2719879150390625, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.29918670654296875, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.4895782470703125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.4895782470703125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.4895782470703125, height: 0.6284723281860352, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.4895782470703125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.4895782470703125, height: 0.5648126602172852, depth: 0.0, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.7340211868286133, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.4351806640625, height: 0.0, depth: 0.17013835906982422, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.4895782470703125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.7071685791015625, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.761566162109375, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.4895782470703125, height: 0.5277786254882812, depth: 0.09722232818603516, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.8837881088256836, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.9925832748413086, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.761566162109375, height: 0.7319440841674805, depth: 0.048610687255859375, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.2719879150390625, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.2719879150390625, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.4895782470703125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.8159637451171875, height: 0.6944437026977539, depth: 0.1944427490234375, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.4895782470703125, height: 0.75, depth: 0.055556297302246094, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.8159637451171875, height: 0.75, depth: 0.055556297302246094, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.761566162109375, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.2719879150390625, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.3807830810546875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.3807830810546875, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.4895782470703125, height: 0.75, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.761566162109375, height: 0.576385498046875, depth: 0.076385498046875, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.2719879150390625, height: 0.09722232818603516, depth: 0.1944437026977539, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.326385498046875, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.2719879150390625, height: 0.09722232818603516, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.4895782470703125, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.4895782470703125, height: 0.644444465637207, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.2719879150390625, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.2719879150390625, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.2719879150390625, height: 0.5, depth: 0.1944437026977539, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.761566162109375, height: 0.3609018325805664, depth: -0.1390981674194336, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.46237945556640625, height: 0.5, depth: 0.1944437026977539, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.46237945556640625, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.761566162109375, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.6933965682983398, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.7071685791015625, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.7477941513061523, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6661977767944336, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6389989852905273, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.768280029296875, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.3532381057739258, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.5030050277709961, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.7612199783325195, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.6118001937866211, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.8972139358520508, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.761566162109375, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.6661977767944336, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.761566162109375, height: 0.6833333969116211, depth: 0.1944437026977539, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7205953598022461, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.543975830078125, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.7071685791015625, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.013599395751953125 }), // 0x56
        Some(TfmChar { width: 1.0060091018676758, height: 0.6833333969116211, depth: 0.0, italic: 0.013599395751953125 }), // 0x57
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.7340211868286133, height: 0.6833333969116211, depth: 0.0, italic: 0.024478912353515625 }), // 0x59
        Some(TfmChar { width: 0.5983734130859375, height: 0.6833333969116211, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.2719879150390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.4895782470703125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.2719879150390625, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.4895782470703125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.2719879150390625, height: 0.6636924743652344, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.2719879150390625, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.543975830078125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.4351806640625, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.543975830078125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.4351806640625, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.29918670654296875, height: 0.6944437026977539, depth: 0.0, italic: 0.0697336196899414 }), // 0x66
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.013599395751953125 }), // 0x67
        Some(TfmChar { width: 0.543975830078125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.2719879150390625, height: 0.6636924743652344, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.29918670654296875, height: 0.6636924743652344, depth: 0.1944437026977539, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.5167770385742188, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.2719879150390625, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.8159637451171875, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.543975830078125, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.543975830078125, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.5167770385742188, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.3807830810546875, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.38622283935546875, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.3807830810546875, height: 0.6150808334350586, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.543975830078125, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.5167770385742188, height: 0.4305562973022461, depth: 0.0, italic: 0.013599395751953125 }), // 0x76
        Some(TfmChar { width: 0.7071685791015625, height: 0.4305562973022461, depth: 0.0, italic: 0.013599395751953125 }), // 0x77
        Some(TfmChar { width: 0.5167770385742188, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5167770385742188, height: 0.4305562973022461, depth: 0.1944437026977539, italic: 0.013599395751953125 }), // 0x79
        Some(TfmChar { width: 0.4351806640625, height: 0.4305562973022461, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.4895782470703125, height: 0.4305562973022461, depth: 0.0, italic: 0.02719879150390625 }), // 0x7B
        Some(TfmChar { width: 0.979156494140625, height: 0.4305562973022461, depth: 0.0, italic: 0.02719879150390625 }), // 0x7C
        Some(TfmChar { width: 0.4895782470703125, height: 0.6944437026977539, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.4895782470703125, height: 0.6636924743652344, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.4895782470703125, height: 0.6636924743652344, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x0B, 0x21, 0.0697336196899414),
        (0x0B, 0x27, 0.0697336196899414),
        (0x0B, 0x29, 0.0697336196899414),
        (0x0B, 0x3F, 0.0697336196899414),
        (0x0B, 0x5D, 0.0697336196899414),
        (0x20, 0x4C, -0.31261348724365234),
        (0x20, 0x6C, -0.2719879150390625),
        (0x27, 0x21, 0.108795166015625),
        (0x27, 0x3F, 0.108795166015625),
        (0x41, 0x43, -0.02719879150390625),
        (0x41, 0x47, -0.02719879150390625),
        (0x41, 0x4F, -0.02719879150390625),
        (0x41, 0x51, -0.02719879150390625),
        (0x41, 0x54, -0.08159637451171875),
        (0x41, 0x55, -0.02719879150390625),
        (0x41, 0x56, -0.108795166015625),
        (0x41, 0x57, -0.108795166015625),
        (0x41, 0x59, -0.08159637451171875),
        (0x41, 0x74, -0.02719879150390625),
        (0x44, 0x41, -0.02719879150390625),
        (0x44, 0x56, -0.02719879150390625),
        (0x44, 0x57, -0.02719879150390625),
        (0x44, 0x58, -0.02719879150390625),
        (0x44, 0x59, -0.02719879150390625),
        (0x46, 0x41, -0.108795166015625),
        (0x46, 0x43, -0.02719879150390625),
        (0x46, 0x47, -0.02719879150390625),
        (0x46, 0x4F, -0.02719879150390625),
        (0x46, 0x51, -0.02719879150390625),
        (0x46, 0x61, -0.08159637451171875),
        (0x46, 0x65, -0.08159637451171875),
        (0x46, 0x6F, -0.08159637451171875),
        (0x46, 0x72, -0.08159637451171875),
        (0x46, 0x75, -0.08159637451171875),
        (0x49, 0x49, 0.02719879150390625),
        (0x4B, 0x43, -0.02719879150390625),
        (0x4B, 0x47, -0.02719879150390625),
        (0x4B, 0x4F, -0.02719879150390625),
        (0x4B, 0x51, -0.02719879150390625),
        (0x4C, 0x54, -0.08159637451171875),
        (0x4C, 0x56, -0.108795166015625),
        (0x4C, 0x57, -0.108795166015625),
        (0x4C, 0x59, -0.08159637451171875),
        (0x4F, 0x41, -0.02719879150390625),
        (0x4F, 0x56, -0.02719879150390625),
        (0x4F, 0x57, -0.02719879150390625),
        (0x4F, 0x58, -0.02719879150390625),
        (0x4F, 0x59, -0.02719879150390625),
        (0x50, 0x2C, -0.08159637451171875),
        (0x50, 0x2E, -0.08159637451171875),
        (0x50, 0x41, -0.08159637451171875),
        (0x50, 0x61, -0.02719879150390625),
        (0x50, 0x65, -0.02719879150390625),
        (0x50, 0x6F, -0.02719879150390625),
        (0x52, 0x43, -0.02719879150390625),
        (0x52, 0x47, -0.02719879150390625),
        (0x52, 0x4F, -0.02719879150390625),
        (0x52, 0x51, -0.02719879150390625),
        (0x52, 0x54, -0.08159637451171875),
        (0x52, 0x55, -0.02719879150390625),
        (0x52, 0x56, -0.108795166015625),
        (0x52, 0x57, -0.108795166015625),
        (0x52, 0x59, -0.08159637451171875),
        (0x52, 0x74, -0.02719879150390625),
        (0x54, 0x41, -0.08159637451171875),
        (0x54, 0x61, -0.08159637451171875),
        (0x54, 0x65, -0.08159637451171875),
        (0x54, 0x6F, -0.08159637451171875),
        (0x54, 0x72, -0.08159637451171875),
        (0x54, 0x75, -0.08159637451171875),
        (0x54, 0x79, -0.02719879150390625),
        (0x56, 0x41, -0.108795166015625),
        (0x56, 0x43, -0.02719879150390625),
        (0x56, 0x47, -0.02719879150390625),
        (0x56, 0x4F, -0.02719879150390625),
        (0x56, 0x51, -0.02719879150390625),
        (0x56, 0x61, -0.08159637451171875),
        (0x56, 0x65, -0.08159637451171875),
        (0x56, 0x6F, -0.08159637451171875),
        (0x56, 0x72, -0.08159637451171875),
        (0x56, 0x75, -0.08159637451171875),
        (0x57, 0x41, -0.108795166015625),
        (0x57, 0x43, -0.02719879150390625),
        (0x57, 0x47, -0.02719879150390625),
        (0x57, 0x4F, -0.02719879150390625),
        (0x57, 0x51, -0.02719879150390625),
        (0x57, 0x61, -0.08159637451171875),
        (0x57, 0x65, -0.08159637451171875),
        (0x57, 0x6F, -0.08159637451171875),
        (0x57, 0x72, -0.08159637451171875),
        (0x57, 0x75, -0.08159637451171875),
        (0x58, 0x43, -0.02719879150390625),
        (0x58, 0x47, -0.02719879150390625),
        (0x58, 0x4F, -0.02719879150390625),
        (0x58, 0x51, -0.02719879150390625),
        (0x59, 0x41, -0.08159637451171875),
        (0x59, 0x61, -0.08159637451171875),
        (0x59, 0x65, -0.08159637451171875),
        (0x59, 0x6F, -0.08159637451171875),
        (0x59, 0x72, -0.08159637451171875),
        (0x59, 0x75, -0.08159637451171875),
        (0x61, 0x6A, 0.0543975830078125),
        (0x61, 0x76, -0.02719879150390625),
        (0x61, 0x77, -0.02719879150390625),
        (0x61, 0x79, -0.02719879150390625),
        (0x62, 0x63, 0.02719879150390625),
        (0x62, 0x64, 0.02719879150390625),
        (0x62, 0x65, 0.02719879150390625),
        (0x62, 0x6A, 0.0543975830078125),
        (0x62, 0x6F, 0.02719879150390625),
        (0x62, 0x71, 0.02719879150390625),
        (0x62, 0x76, -0.02719879150390625),
        (0x62, 0x77, -0.02719879150390625),
        (0x62, 0x78, -0.02719879150390625),
        (0x62, 0x79, -0.02719879150390625),
        (0x63, 0x68, -0.02719879150390625),
        (0x63, 0x6B, -0.02719879150390625),
        (0x66, 0x21, 0.0697336196899414),
        (0x66, 0x27, 0.0697336196899414),
        (0x66, 0x29, 0.0697336196899414),
        (0x66, 0x3F, 0.0697336196899414),
        (0x66, 0x5D, 0.0697336196899414),
        (0x67, 0x6A, 0.02719879150390625),
        (0x68, 0x62, -0.02719879150390625),
        (0x68, 0x74, -0.02719879150390625),
        (0x68, 0x75, -0.02719879150390625),
        (0x68, 0x76, -0.02719879150390625),
        (0x68, 0x77, -0.02719879150390625),
        (0x68, 0x79, -0.02719879150390625),
        (0x6B, 0x61, -0.0543975830078125),
        (0x6B, 0x63, -0.02719879150390625),
        (0x6B, 0x65, -0.02719879150390625),
        (0x6B, 0x6F, -0.02719879150390625),
        (0x6D, 0x62, -0.02719879150390625),
        (0x6D, 0x74, -0.02719879150390625),
        (0x6D, 0x75, -0.02719879150390625),
        (0x6D, 0x76, -0.02719879150390625),
        (0x6D, 0x77, -0.02719879150390625),
        (0x6D, 0x79, -0.02719879150390625),
        (0x6E, 0x62, -0.02719879150390625),
        (0x6E, 0x74, -0.02719879150390625),
        (0x6E, 0x75, -0.02719879150390625),
        (0x6E, 0x76, -0.02719879150390625),
        (0x6E, 0x77, -0.02719879150390625),
        (0x6E, 0x79, -0.02719879150390625),
        (0x6F, 0x63, 0.02719879150390625),
        (0x6F, 0x64, 0.02719879150390625),
        (0x6F, 0x65, 0.02719879150390625),
        (0x6F, 0x6A, 0.0543975830078125),
        (0x6F, 0x6F, 0.02719879150390625),
        (0x6F, 0x71, 0.02719879150390625),
        (0x6F, 0x76, -0.02719879150390625),
        (0x6F, 0x77, -0.02719879150390625),
        (0x6F, 0x78, -0.02719879150390625),
        (0x6F, 0x79, -0.02719879150390625),
        (0x70, 0x63, 0.02719879150390625),
        (0x70, 0x64, 0.02719879150390625),
        (0x70, 0x65, 0.02719879150390625),
        (0x70, 0x6A, 0.0543975830078125),
        (0x70, 0x6F, 0.02719879150390625),
        (0x70, 0x71, 0.02719879150390625),
        (0x70, 0x76, -0.02719879150390625),
        (0x70, 0x77, -0.02719879150390625),
        (0x70, 0x78, -0.02719879150390625),
        (0x70, 0x79, -0.02719879150390625),
        (0x74, 0x77, -0.02719879150390625),
        (0x74, 0x79, -0.02719879150390625),
        (0x75, 0x77, -0.02719879150390625),
        (0x76, 0x61, -0.0543975830078125),
        (0x76, 0x63, -0.02719879150390625),
        (0x76, 0x65, -0.02719879150390625),
        (0x76, 0x6F, -0.02719879150390625),
        (0x77, 0x61, -0.02719879150390625),
        (0x77, 0x63, -0.02719879150390625),
        (0x77, 0x65, -0.02719879150390625),
        (0x77, 0x6F, -0.02719879150390625),
        (0x79, 0x2C, -0.08159637451171875),
        (0x79, 0x2E, -0.08159637451171875),
        (0x79, 0x61, -0.02719879150390625),
        (0x79, 0x65, -0.02719879150390625),
        (0x79, 0x6F, -0.02719879150390625),
    ],
};

/// `cmr17.tfm`.
#[rustfmt::skip]
pub static CMR17: TfmFont = TfmFont {
    name: "cmr17",
    space: 0.30188751220703125,
    skew_char: None,
    params: &[0.0, 0.30188751220703125, 0.15673065185546875, 0.10448741912841797, 0.43049144744873047, 0.9172372817993164, 0.10448741912841797],
    chars: &[
        Some(TfmChar { width: 0.5759658813476562, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.7720804214477539, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.7198371887207031, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.6410703659057617, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.6153497695922852, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.6933145523071289, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.6675930023193359, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.7198371887207031, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.6675930023193359, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.7198371887207031, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.6675930023193359, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.5254096984863281, height: 0.6944446563720703, depth: 0.0, italic: 0.059276580810546875 }), // 0x0B
        Some(TfmChar { width: 0.49928760528564453, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.49928760528564453, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.7489309310913086, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.7489309310913086, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.24964332580566406, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.27576541900634766, height: 0.43049144744873047, depth: 0.19450855255126953, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.4586181640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.4586181640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.4586181640625, height: 0.6284561157226562, depth: 0.0, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.4586181640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.4586181640625, height: 0.559539794921875, depth: 0.0, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.6933145523071289, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.4063749313354492, height: 0.0, depth: 0.1701955795288086, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.4586181640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.6675930023193359, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.7198371887207031, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.4586181640625, height: 0.5277462005615234, depth: 0.09725475311279297, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.8371849060058594, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.9416723251342773, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.7198371887207031, height: 0.7318201065063477, depth: 0.04862785339355469, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.24964332580566406, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.24964332580566406, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.4586181640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.7720804214477539, height: 0.6944446563720703, depth: 0.19450855255126953, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.4586181640625, height: 0.7490997314453125, depth: 0.05465507507324219, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.7720804214477539, height: 0.7490997314453125, depth: 0.05465507507324219, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.7198371887207031, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.24964332580566406, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.35413074493408203, height: 0.7490997314453125, depth: 0.24916362762451172, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.35413074493408203, height: 0.7490997314453125, depth: 0.24916362762451172, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.4586181640625, height: 0.7490997314453125, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.7198371887207031, height: 0.5634298324584961, depth: 0.06349468231201172, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.24964332580566406, height: 0.08198356628417969, depth: 0.19450855255126953, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.30188751220703125, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.24964332580566406, height: 0.08198356628417969, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.4586181640625, height: 0.7490997314453125, depth: 0.24916362762451172, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.4586181640625, height: 0.6446113586425781, depth: 0.0, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.24964332580566406, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.24964332580566406, height: 0.43049144744873047, depth: 0.19450855255126953, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.24964332580566406, height: 0.4999351501464844, depth: 0.19450855255126953, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.7198371887207031, height: 0.3555784225463867, depth: -0.14435672760009766, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.4324970245361328, height: 0.4999351501464844, depth: 0.19450855255126953, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.4324970245361328, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.7198371887207031, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.6933145523071289, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.654332160949707, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.6675930023193359, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.7065753936767578, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6282100677490234, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6020879745483398, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.7262668609619141, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.6933145523071289, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.3276081085205078, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.4714794158935547, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.7194356918334961, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.5759658813476562, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.8500452041625977, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.6933145523071289, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.7198371887207031, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.6282100677490234, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.7198371887207031, height: 0.683192253112793, depth: 0.19450855255126953, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.6804533004760742, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.5108623504638672, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.6675930023193359, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.6933145523071289, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.6933145523071289, height: 0.683192253112793, depth: 0.0, italic: 0.013060569763183594 }), // 0x56
        Some(TfmChar { width: 0.9545326232910156, height: 0.683192253112793, depth: 0.0, italic: 0.013060569763183594 }), // 0x57
        Some(TfmChar { width: 0.6933145523071289, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.6933145523071289, height: 0.683192253112793, depth: 0.0, italic: 0.023509979248046875 }), // 0x59
        Some(TfmChar { width: 0.563105583190918, height: 0.683192253112793, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.24964332580566406, height: 0.7490997314453125, depth: 0.24916362762451172, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.4586181640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.24964332580566406, height: 0.7490997314453125, depth: 0.24916362762451172, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.4586181640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.24964332580566406, height: 0.655980110168457, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.24964332580566406, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.4586181640625, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.5108623504638672, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.4063749313354492, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.5108623504638672, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.4063749313354492, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.27576541900634766, height: 0.6944446563720703, depth: 0.0, italic: 0.059276580810546875 }), // 0x66
        Some(TfmChar { width: 0.4586181640625, height: 0.43049144744873047, depth: 0.19450855255126953, italic: 0.013060569763183594 }), // 0x67
        Some(TfmChar { width: 0.5108623504638672, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.24964332580566406, height: 0.655980110168457, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.27576541900634766, height: 0.655980110168457, depth: 0.19450855255126953, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.4847402572631836, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.24964332580566406, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.7720804214477539, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.5108623504638672, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.4586181640625, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5108623504638672, height: 0.43049144744873047, depth: 0.19450855255126953, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.4847402572631836, height: 0.43049144744873047, depth: 0.19450855255126953, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.35413074493408203, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.3593559265136719, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.35413074493408203, height: 0.6149883270263672, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.5108623504638672, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.4847402572631836, height: 0.43049144744873047, depth: 0.0, italic: 0.013060569763183594 }), // 0x76
        Some(TfmChar { width: 0.6675930023193359, height: 0.43049144744873047, depth: 0.0, italic: 0.013060569763183594 }), // 0x77
        Some(TfmChar { width: 0.4847402572631836, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.4847402572631836, height: 0.43049144744873047, depth: 0.19450855255126953, italic: 0.013060569763183594 }), // 0x79
        Some(TfmChar { width: 0.4063749313354492, height: 0.43049144744873047, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.4586181640625, height: 0.43049144744873047, depth: 0.0, italic: 0.026122093200683594 }), // 0x7B
        Some(TfmChar { width: 0.9172372817993164, height: 0.43049144744873047, depth: 0.0, italic: 0.026122093200683594 }), // 0x7C
        Some(TfmChar { width: 0.4586181640625, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.4586181640625, height: 0.655980110168457, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.4586181640625, height: 0.655980110168457, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
        (0x0B, 0x21, 0.059276580810546875),
        (0x0B, 0x27, 0.059276580810546875),
        (0x0B, 0x29, 0.059276580810546875),
        (0x0B, 0x3F, 0.059276580810546875),
        (0x0B, 0x5D, 0.059276580810546875),
        (0x20, 0x4C, -0.28862571716308594),
        (0x20, 0x6C, -0.24964332580566406),
        (0x27, 0x21, 0.10448741912841797),
        (0x27, 0x3F, 0.10448741912841797),
        (0x41, 0x43, -0.026122093200683594),
        (0x41, 0x47, -0.026122093200683594),
        (0x41, 0x4F, -0.026122093200683594),
        (0x41, 0x51, -0.026122093200683594),
        (0x41, 0x54, -0.07836532592773438),
        (0x41, 0x55, -0.026122093200683594),
        (0x41, 0x56, -0.10448741912841797),
        (0x41, 0x57, -0.10448741912841797),
        (0x41, 0x59, -0.07836532592773438),
        (0x41, 0x74, -0.026122093200683594),
        (0x44, 0x41, -0.026122093200683594),
        (0x44, 0x56, -0.026122093200683594),
        (0x44, 0x57, -0.026122093200683594),
        (0x44, 0x58, -0.026122093200683594),
        (0x44, 0x59, -0.026122093200683594),
        (0x46, 0x41, -0.10448741912841797),
        (0x46, 0x43, -0.026122093200683594),
        (0x46, 0x47, -0.026122093200683594),
        (0x46, 0x4F, -0.026122093200683594),
        (0x46, 0x51, -0.026122093200683594),
        (0x46, 0x61, -0.07836532592773438),
        (0x46, 0x65, -0.07836532592773438),
        (0x46, 0x6F, -0.07836532592773438),
        (0x46, 0x72, -0.07836532592773438),
        (0x46, 0x75, -0.07836532592773438),
        (0x49, 0x49, 0.026122093200683594),
        (0x4B, 0x43, -0.026122093200683594),
        (0x4B, 0x47, -0.026122093200683594),
        (0x4B, 0x4F, -0.026122093200683594),
        (0x4B, 0x51, -0.026122093200683594),
        (0x4C, 0x54, -0.07836532592773438),
        (0x4C, 0x56, -0.10448741912841797),
        (0x4C, 0x57, -0.10448741912841797),
        (0x4C, 0x59, -0.07836532592773438),
        (0x4F, 0x41, -0.026122093200683594),
        (0x4F, 0x56, -0.026122093200683594),
        (0x4F, 0x57, -0.026122093200683594),
        (0x4F, 0x58, -0.026122093200683594),
        (0x4F, 0x59, -0.026122093200683594),
        (0x50, 0x2C, -0.07836532592773438),
        (0x50, 0x2E, -0.07836532592773438),
        (0x50, 0x41, -0.07836532592773438),
        (0x50, 0x61, -0.026122093200683594),
        (0x50, 0x65, -0.026122093200683594),
        (0x50, 0x6F, -0.026122093200683594),
        (0x52, 0x43, -0.026122093200683594),
        (0x52, 0x47, -0.026122093200683594),
        (0x52, 0x4F, -0.026122093200683594),
        (0x52, 0x51, -0.026122093200683594),
        (0x52, 0x54, -0.07836532592773438),
        (0x52, 0x55, -0.026122093200683594),
        (0x52, 0x56, -0.10448741912841797),
        (0x52, 0x57, -0.10448741912841797),
        (0x52, 0x59, -0.07836532592773438),
        (0x52, 0x74, -0.026122093200683594),
        (0x54, 0x41, -0.07836532592773438),
        (0x54, 0x61, -0.07836532592773438),
        (0x54, 0x65, -0.07836532592773438),
        (0x54, 0x6F, -0.07836532592773438),
        (0x54, 0x72, -0.07836532592773438),
        (0x54, 0x75, -0.07836532592773438),
        (0x54, 0x79, -0.026122093200683594),
        (0x56, 0x41, -0.10448741912841797),
        (0x56, 0x43, -0.026122093200683594),
        (0x56, 0x47, -0.026122093200683594),
        (0x56, 0x4F, -0.026122093200683594),
        (0x56, 0x51, -0.026122093200683594),
        (0x56, 0x61, -0.07836532592773438),
        (0x56, 0x65, -0.07836532592773438),
        (0x56, 0x6F, -0.07836532592773438),
        (0x56, 0x72, -0.07836532592773438),
        (0x56, 0x75, -0.07836532592773438),
        (0x57, 0x41, -0.10448741912841797),
        (0x57, 0x43, -0.026122093200683594),
        (0x57, 0x47, -0.026122093200683594),
        (0x57, 0x4F, -0.026122093200683594),
        (0x57, 0x51, -0.026122093200683594),
        (0x57, 0x61, -0.07836532592773438),
        (0x57, 0x65, -0.07836532592773438),
        (0x57, 0x6F, -0.07836532592773438),
        (0x57, 0x72, -0.07836532592773438),
        (0x57, 0x75, -0.07836532592773438),
        (0x58, 0x43, -0.026122093200683594),
        (0x58, 0x47, -0.026122093200683594),
        (0x58, 0x4F, -0.026122093200683594),
        (0x58, 0x51, -0.026122093200683594),
        (0x59, 0x41, -0.07836532592773438),
        (0x59, 0x61, -0.07836532592773438),
        (0x59, 0x65, -0.07836532592773438),
        (0x59, 0x6F, -0.07836532592773438),
        (0x59, 0x72, -0.07836532592773438),
        (0x59, 0x75, -0.07836532592773438),
        (0x61, 0x6A, 0.05224323272705078),
        (0x61, 0x76, -0.026122093200683594),
        (0x61, 0x77, -0.026122093200683594),
        (0x61, 0x79, -0.026122093200683594),
        (0x62, 0x63, 0.026122093200683594),
        (0x62, 0x64, 0.026122093200683594),
        (0x62, 0x65, 0.026122093200683594),
        (0x62, 0x6A, 0.05224323272705078),
        (0x62, 0x6F, 0.026122093200683594),
        (0x62, 0x71, 0.026122093200683594),
        (0x62, 0x76, -0.026122093200683594),
        (0x62, 0x77, -0.026122093200683594),
        (0x62, 0x78, -0.026122093200683594),
        (0x62, 0x79, -0.026122093200683594),
        (0x63, 0x68, -0.026122093200683594),
        (0x63, 0x6B, -0.026122093200683594),
        (0x66, 0x21, 0.059276580810546875),
        (0x66, 0x27, 0.059276580810546875),
        (0x66, 0x29, 0.059276580810546875),
        (0x66, 0x3F, 0.059276580810546875),
        (0x66, 0x5D, 0.059276580810546875),
        (0x67, 0x6A, 0.026122093200683594),
        (0x68, 0x62, -0.026122093200683594),
        (0x68, 0x74, -0.026122093200683594),
        (0x68, 0x75, -0.026122093200683594),
        (0x68, 0x76, -0.026122093200683594),
        (0x68, 0x77, -0.026122093200683594),
        (0x68, 0x79, -0.026122093200683594),
        (0x6B, 0x61, -0.05224323272705078),
        (0x6B, 0x63, -0.026122093200683594),
        (0x6B, 0x65, -0.026122093200683594),
        (0x6B, 0x6F, -0.026122093200683594),
        (0x6D, 0x62, -0.026122093200683594),
        (0x6D, 0x74, -0.026122093200683594),
        (0x6D, 0x75, -0.026122093200683594),
        (0x6D, 0x76, -0.026122093200683594),
        (0x6D, 0x77, -0.026122093200683594),
        (0x6D, 0x79, -0.026122093200683594),
        (0x6E, 0x62, -0.026122093200683594),
        (0x6E, 0x74, -0.026122093200683594),
        (0x6E, 0x75, -0.026122093200683594),
        (0x6E, 0x76, -0.026122093200683594),
        (0x6E, 0x77, -0.026122093200683594),
        (0x6E, 0x79, -0.026122093200683594),
        (0x6F, 0x63, 0.026122093200683594),
        (0x6F, 0x64, 0.026122093200683594),
        (0x6F, 0x65, 0.026122093200683594),
        (0x6F, 0x6A, 0.05224323272705078),
        (0x6F, 0x6F, 0.026122093200683594),
        (0x6F, 0x71, 0.026122093200683594),
        (0x6F, 0x76, -0.026122093200683594),
        (0x6F, 0x77, -0.026122093200683594),
        (0x6F, 0x78, -0.026122093200683594),
        (0x6F, 0x79, -0.026122093200683594),
        (0x70, 0x63, 0.026122093200683594),
        (0x70, 0x64, 0.026122093200683594),
        (0x70, 0x65, 0.026122093200683594),
        (0x70, 0x6A, 0.05224323272705078),
        (0x70, 0x6F, 0.026122093200683594),
        (0x70, 0x71, 0.026122093200683594),
        (0x70, 0x76, -0.026122093200683594),
        (0x70, 0x77, -0.026122093200683594),
        (0x70, 0x78, -0.026122093200683594),
        (0x70, 0x79, -0.026122093200683594),
        (0x74, 0x77, -0.026122093200683594),
        (0x74, 0x79, -0.026122093200683594),
        (0x75, 0x77, -0.026122093200683594),
        (0x76, 0x61, -0.05224323272705078),
        (0x76, 0x63, -0.026122093200683594),
        (0x76, 0x65, -0.026122093200683594),
        (0x76, 0x6F, -0.026122093200683594),
        (0x77, 0x61, -0.026122093200683594),
        (0x77, 0x63, -0.026122093200683594),
        (0x77, 0x65, -0.026122093200683594),
        (0x77, 0x6F, -0.026122093200683594),
        (0x79, 0x2C, -0.07836532592773438),
        (0x79, 0x2E, -0.07836532592773438),
        (0x79, 0x61, -0.026122093200683594),
        (0x79, 0x65, -0.026122093200683594),
        (0x79, 0x6F, -0.026122093200683594),
    ],
};

/// `msam10.tfm`.
#[rustfmt::skip]
pub static MSAM10: TfmFont = TfmFont {
    name: "msam10",
    space: 0.0,
    skew_char: None,
    params: &[0.0, 0.0, 0.0, 0.0, 0.4305553436279297, 1.0000028610229492, 0.0, 0.6765079498291016, 0.39373207092285156, 0.44373130798339844, 0.6859512329101562, 0.34484100341796875, 0.41289234161376953, 0.36289215087890625, 0.28888893127441406, 0.14999961853027344, 0.24721717834472656, 0.3861083984375, 0.05000019073486328, 2.3899993896484375, 1.010000228881836, 0.25],
    chars: &[
        Some(TfmChar { width: 0.7777805328369141, height: 0.6750001907348633, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.7777805328369141, height: 0.6750001907348633, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.7777805328369141, height: 0.6750001907348633, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.7777805328369141, height: 0.6750001907348633, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.7777805328369141, height: 0.6750001907348633, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.27777862548828125, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.6666688919067383, height: 0.6922407150268555, depth: 0.11111164093017578, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.6666688919067383, height: 0.6922407150268555, depth: 0.11111164093017578, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.7777805328369141, height: 0.5819778442382812, depth: 0.08197784423828125, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.7777805328369141, height: 0.5819778442382812, depth: 0.08197784423828125, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 1.0000028610229492, height: 0.5223941802978516, depth: 0.013539314270019531, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 1.0000028610229492, height: 0.5223941802978516, depth: 0.013539314270019531, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.7777805328369141, height: 0.6750001907348633, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.722224235534668, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.8888912200927734, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.6111125946044922, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 1.0000028610229492, height: 0.5223941802978516, depth: 0.013539314270019531, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 1.0000028610229492, height: 0.5223941802978516, depth: 0.013539314270019531, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 1.0000028610229492, height: 0.6750001907348633, depth: 0.1808013916015625, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 1.0000028610229492, height: 0.6750001907348633, depth: 0.1808013916015625, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.8333358764648438, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.8333358764648438, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.4166688919067383, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.4166688919067383, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.4166688919067383, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.4166688919067383, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 1.111114501953125, height: 0.5223941802978516, depth: 0.013539314270019531, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 1.111114501953125, height: 0.5223941802978516, depth: 0.013539314270019531, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.0000028610229492, height: 0.6750001907348633, depth: 0.1808013916015625, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.0000028610229492, height: 0.6750001907348633, depth: 0.1808013916015625, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.5000019073486328, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.5000019073486328, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.0000028610229492, height: 0.3778820037841797, depth: -0.13312530517578125, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.3888931274414062, height: 0.3778820037841797, depth: -0.13312530517578125, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 1.0000028610229492, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 1.0000028610229492, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.7777805328369141, height: 0.7295808792114258, depth: 0.22958087921142578, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.7777805328369141, height: 0.7295808792114258, depth: 0.22958087921142578, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.7777805328369141, height: 0.7295808792114258, depth: 0.22958087921142578, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.7777805328369141, height: 0.755833625793457, depth: 0.25583362579345703, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.111114501953125, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.6666688919067383, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.6666688919067383, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.7777805328369141, height: 0.5819778442382812, depth: 0.08197784423828125, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.7777805328369141, height: 0.9166688919067383, depth: 0.08197784423828125, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.7777805328369141, height: 0.7295808792114258, depth: 0.22958087921142578, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.7777805328369141, height: 0.7295808792114258, depth: 0.22958087921142578, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.7777805328369141, height: 0.755833625793457, depth: 0.25583362579345703, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.7777805328369141, height: 0.755833625793457, depth: 0.25583362579345703, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.7777805328369141, height: 0.6750001907348633, depth: 0.1808013916015625, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.27499961853027344, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5000019073486328, height: 0.4640169143676758, depth: -0.03598308563232422, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.7777805328369141, height: 0.5819778442382812, depth: 0.08197784423828125, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.7777805328369141, height: 0.5819778442382812, depth: 0.08197784423828125, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.7777805328369141, height: 0.755833625793457, depth: 0.25583362579345703, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.7777805328369141, height: 0.6750001907348633, depth: 0.1808013916015625, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.7777805328369141, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.9444456100463867, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.5000019073486328, height: 0.755833625793457, depth: 0.25583362579345703, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.722224235534668, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.5000019073486328, height: 0.4640169143676758, depth: -0.03598308563232422, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.5000019073486328, height: 0.4640169143676758, depth: -0.03598308563232422, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.722224235534668, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.722224235534668, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.722224235534668, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.7777805328369141, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.7777805328369141, height: 0.8856906890869141, depth: 0.38569068908691406, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7777805328369141, height: 0.8856906890869141, depth: 0.38569068908691406, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.7777805328369141, height: 0.9825649261474609, depth: 0.48256492614746094, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.7777805328369141, height: 0.9825649261474609, depth: 0.48256492614746094, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.7500019073486328, height: 0.6750001907348633, depth: 0.0, italic: 0.024999618530273438 }), // 0x55
        Some(TfmChar { width: 1.0000028610229492, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 1.0000028610229492, height: 0.6366653442382812, depth: 0.13666534423828125, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 0.8333358764648438, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.6111125946044922, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.6111125946044922, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.6111125946044922, height: 0.755833625793457, depth: 0.1944427490234375, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.722224235534668, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.722224235534668, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.722224235534668, height: 0.5223941802978516, depth: 0.03517341613769531, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.7777805328369141, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.7777805328369141, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.7777805328369141, height: 0.6922407150268555, depth: 0.1944427490234375, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.6666688919067383, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.6666688919067383, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.7604169845581055, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.7604169845581055, height: 0.5498600006103516, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.7777805328369141, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.7777805328369141, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.7777805328369141, height: 0.755833625793457, depth: 0.25583362579345703, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.7777805328369141, height: 0.755833625793457, depth: 0.25583362579345703, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.0606231689453125, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.7777805328369141, height: 0.5498600006103516, depth: 0.0606231689453125, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 1.3333377838134766, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 1.3333377838134766, height: 0.5498600006103516, depth: 0.03517341613769531, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5000019073486328, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.5000019073486328, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.9466629028320312, height: 0.6922407150268555, depth: 0.15558815002441406, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.9022188186645508, height: 0.6922407150268555, depth: 0.15558815002441406, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.6666688919067383, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.7777805328369141, height: 0.6922407150268555, depth: 0.08197784423828125, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.7777805328369141, height: 0.3778820037841797, depth: -0.13312530517578125, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.7777805328369141, height: 0.4640169143676758, depth: -0.03598308563232422, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.5000019073486328, height: 0.3778820037841797, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5000019073486328, height: 0.3778820037841797, depth: 0.0, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.8333358764648438, height: 0.6922407150268555, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.5000019073486328, height: 0.8250045776367188, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.5555572509765625, height: 0.4305553436279297, depth: 0.1944427490234375, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.7777805328369141, height: 0.5819778442382812, depth: 0.08197784423828125, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.7777805328369141, height: 0.5819778442382812, depth: 0.08197784423828125, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.7777805328369141, height: 0.5819778442382812, depth: 0.08197784423828125, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `msam7.tfm`.
#[rustfmt::skip]
pub static MSAM7: TfmFont = TfmFont {
    name: "msam7",
    space: 0.0,
    skew_char: None,
    params: &[0.0, 0.0, 0.0, 0.0, 0.4305553436279297, 1.1706409454345703, 0.0, 0.7323131561279297, 0.38390254974365234, 0.4712047576904297, 0.7521495819091797, 0.34421443939208984, 0.5025434494018555, 0.43111515045166016, 0.28571414947509766, 0.14285755157470703, 0.28571414947509766, 0.35317134857177734, 0.07142829895019531, 1.6999988555908203, 1.1571435928344727, 0.25],
    chars: &[
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.33928775787353516, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.7698450088500977, height: 0.6888885498046875, depth: 0.12349796295166016, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.7698450088500977, height: 0.6888885498046875, depth: 0.12349796295166016, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.8928613662719727, height: 0.6113567352294922, depth: 0.12349796295166016, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.8928613662719727, height: 0.6113567352294922, depth: 0.12349796295166016, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 1.1388940811157227, height: 0.5288543701171875, depth: 0.0288543701171875, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 1.1388940811157227, height: 0.5288543701171875, depth: 0.0288543701171875, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.8313531875610352, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 1.0158777236938477, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.7083368301391602, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 1.1388940811157227, height: 0.5288543701171875, depth: 0.0288543701171875, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 1.1388940811157227, height: 0.5288543701171875, depth: 0.0288543701171875, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 1.1388940811157227, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 1.1388940811157227, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.9543695449829102, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.9543695449829102, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.4930591583251953, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.4930591583251953, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.4930591583251953, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.4930591583251953, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 1.2619104385375977, height: 0.5288543701171875, depth: 0.0288543701171875, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 1.2619104385375977, height: 0.5288543701171875, depth: 0.0288543701171875, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.1388940811157227, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.1388940811157227, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.5853204727172852, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.5853204727172852, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.5694513320922852, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 1.1388940811157227, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 1.1388940811157227, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.8928613662719727, height: 0.78582763671875, depth: 0.29163646697998047, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.8928613662719727, height: 0.78582763671875, depth: 0.29163646697998047, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.8928613662719727, height: 0.78582763671875, depth: 0.29163646697998047, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.32991886138916016, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.2619104385375977, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.7698450088500977, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.7698450088500977, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.8928613662719727, height: 0.6113567352294922, depth: 0.12349796295166016, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.861114501953125, height: 0.98809814453125, depth: 0.12349796295166016, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.8928613662719727, height: 0.78582763671875, depth: 0.27872371673583984, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.8928613662719727, height: 0.78582763671875, depth: 0.27872371673583984, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.32991886138916016, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.24509334564208984, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.32936573028564453, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5535736083984375, height: 0.4905567169189453, depth: -0.009443283081054688, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.8928613662719727, height: 0.6279497146606445, depth: 0.12349796295166016, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.8928613662719727, height: 0.6279497146606445, depth: 0.12349796295166016, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.24509334564208984, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.8928613662719727, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.8928613662719727, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.8928613662719727, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.8928613662719727, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.982147216796875, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.5853204727172852, height: 0.744049072265625, depth: 0.24509334564208984, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.7718286514282227, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.8928613662719727, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.8928613662719727, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.5535736083984375, height: 0.4905567169189453, depth: -0.009443283081054688, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.5535736083984375, height: 0.4905567169189453, depth: -0.009443283081054688, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.7718286514282227, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.7718286514282227, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.7718286514282227, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.8928613662719727, height: 0.9664459228515625, depth: 0.4664459228515625, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.8928613662719727, height: 0.9664459228515625, depth: 0.4664459228515625, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.8928613662719727, height: 1.0762395858764648, depth: 0.5762395858764648, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.8928613662719727, height: 1.0762395858764648, depth: 0.5762395858764648, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.8591327667236328, height: 0.6888885498046875, depth: 0.0, italic: 0.027679443359375 }), // 0x55
        Some(TfmChar { width: 1.1388940811157227, height: 0.66143798828125, depth: 0.1658802032470703, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 1.1388940811157227, height: 0.66143798828125, depth: 0.1658802032470703, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 0.9543695449829102, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.7083368301391602, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.7083368301391602, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.7083368301391602, height: 0.744049072265625, depth: 0.1899566650390625, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 0.8313531875610352, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 0.8313531875610352, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.8313531875610352, height: 0.5288543701171875, depth: 0.0288543701171875, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.8928613662719727, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.8928613662719727, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.7698450088500977, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.7698450088500977, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.880558967590332, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.880558967590332, height: 0.5656147003173828, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.8928613662719727, height: 0.6113567352294922, depth: 0.10366344451904297, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.8928613662719727, height: 0.6113567352294922, depth: 0.10366344451904297, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 1.5079431533813477, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 1.5079431533813477, height: 0.5656147003173828, depth: 0.07567501068115234, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5853204727172852, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.5853204727172852, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.9900751113891602, height: 0.66143798828125, depth: 0.1658802032470703, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.9444408416748047, height: 0.66143798828125, depth: 0.1658802032470703, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.7698450088500977, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.8928613662719727, height: 0.744049072265625, depth: 0.12349796295166016, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.8928613662719727, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.8928613662719727, height: 0.4905567169189453, depth: -0.009443283081054688, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5853204727172852, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.9543695449829102, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.5853204727172852, height: 0.8247308731079102, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.6468286514282227, height: 0.4305553436279297, depth: 0.1899566650390625, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.8928613662719727, height: 0.6113567352294922, depth: 0.12349796295166016, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.8928613662719727, height: 0.6113567352294922, depth: 0.12349796295166016, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.8928613662719727, height: 0.6113567352294922, depth: 0.12349796295166016, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `msam5.tfm`.
#[rustfmt::skip]
pub static MSAM5: TfmFont = TfmFont {
    name: "msam5",
    space: 0.0,
    skew_char: None,
    params: &[0.0, 0.0, 0.0, 0.0, 0.4305543899536133, 1.4722414016723633, 0.0, 0.9252414703369141, 0.3874635696411133, 0.5041322708129883, 1.0252408981323242, 0.5319089889526367, 0.5035552978515625, 0.40355491638183594, 0.2944459915161133, 0.19999980926513672, 0.39999961853027344, 0.49444580078125, 0.10000038146972656, 1.9799985885620117, 1.4200010299682617, 0.25],
    chars: &[
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.0, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.0, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.45833778381347656, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.9444580078125, height: 0.6805601119995117, depth: 0.13605022430419922, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.9444580078125, height: 0.6805601119995117, depth: 0.13605022430419922, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.16394329071044922, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.16394329071044922, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 1.3611326217651367, height: 0.5434017181396484, depth: 0.03316020965576172, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 1.3611326217651367, height: 0.5434017181396484, depth: 0.03316020965576172, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.0, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 1.0139036178588867, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 1.2222414016723633, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.8750123977661133, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 1.3611326217651367, height: 0.5434017181396484, depth: 0.03316020965576172, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 1.3611326217651367, height: 0.5434017181396484, depth: 0.03316020965576172, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 1.3611326217651367, height: 0.7167024612426758, depth: 0.19507789611816406, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 1.3611326217651367, height: 0.7167024612426758, depth: 0.19507789611816406, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 1.1527957916259766, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 1.1527957916259766, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.6319522857666016, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.6319522857666016, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.6319522857666016, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.6319522857666016, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 1.5000247955322266, height: 0.5434017181396484, depth: 0.03316020965576172, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 1.5000247955322266, height: 0.5434017181396484, depth: 0.03316020965576172, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.3611326217651367, height: 0.7167024612426758, depth: 0.19507789611816406, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.3611326217651367, height: 0.7167024612426758, depth: 0.19507789611816406, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.7361202239990234, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.7361202239990234, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.8472537994384766, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 1.3611326217651367, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 1.3611326217651367, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.0833492279052734, height: 0.8520717620849609, depth: 0.33591270446777344, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.33591270446777344, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.33591270446777344, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 1.0833492279052734, height: 0.8771305084228516, depth: 0.3714265823364258, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.5000247955322266, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.9444580078125, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.9444580078125, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.16394329071044922, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.9722414016723633, height: 1.0833492279052734, depth: 0.16394329071044922, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.309722900390625, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.309722900390625, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 1.0833492279052734, height: 0.8771305084228516, depth: 0.3714265823364258, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 1.0833492279052734, height: 0.8520717620849609, depth: 0.3714265823364258, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 1.0833492279052734, height: 0.7651185989379883, depth: 0.2802400588989258, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.4409761428833008, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.6250123977661133, height: 0.5019893646240234, depth: 0.016332626342773438, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 1.0833492279052734, height: 0.6472110748291016, depth: 0.16394329071044922, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 1.0833492279052734, height: 0.6472110748291016, depth: 0.16394329071044922, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 1.0833492279052734, height: 0.8520717620849609, depth: 0.3714265823364258, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 1.0833492279052734, height: 0.7651185989379883, depth: 0.2802400588989258, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 1.0833492279052734, height: 0.7167024612426758, depth: 0.21932029724121094, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 1.0694456100463867, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.7361202239990234, height: 0.7651185989379883, depth: 0.25, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.8750028610229492, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.6250123977661133, height: 0.5019893646240234, depth: 0.016332626342773438, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.6250123977661133, height: 0.5019893646240234, depth: 0.016332626342773438, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.8750028610229492, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.8750028610229492, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.8750028610229492, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 1.0833492279052734, height: 1.0211362838745117, depth: 0.5211362838745117, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 1.0833492279052734, height: 1.0211362838745117, depth: 0.5211362838745117, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 1.0833492279052734, height: 1.1395416259765625, depth: 0.6395416259765625, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 1.0833492279052734, height: 1.1395416259765625, depth: 0.6395416259765625, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 1.0361270904541016, height: 0.6805601119995117, depth: 0.0, italic: 0.03125 }), // 0x55
        Some(TfmChar { width: 1.3611326217651367, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 1.3611326217651367, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 1.1527957916259766, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.8750123977661133, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.8750123977661133, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.8750123977661133, height: 0.7651185989379883, depth: 0.19507789611816406, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 1.0139036178588867, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 1.0139036178588867, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 1.0139036178588867, height: 0.5434017181396484, depth: 0.03316020965576172, italic: 0.0 }), // 0x5E
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x5F
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.19507789611816406, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.9444580078125, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.9444580078125, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 1.0347232818603516, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 1.0347232818603516, height: 0.5434017181396484, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 1.0833492279052734, height: 0.8520717620849609, depth: 0.3714265823364258, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 1.0833492279052734, height: 0.8520717620849609, depth: 0.3714265823364258, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 1.0833492279052734, height: 0.6472110748291016, depth: 0.13605022430419922, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 1.0833492279052734, height: 0.6472110748291016, depth: 0.13605022430419922, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 1.7778072357177734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 1.7778072357177734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.7361202239990234, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.7361202239990234, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 1.0361270904541016, height: 0.7167024612426758, depth: 0.16394329071044922, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.9889039993286133, height: 0.7167024612426758, depth: 0.16394329071044922, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.9444580078125, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 1.0833492279052734, height: 0.8194856643676758, depth: 0.16394329071044922, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 1.0833492279052734, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 1.0833492279052734, height: 0.5019893646240234, depth: 0.016332626342773438, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.7361202239990234, height: 0.5019893646240234, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.7361202239990234, height: 0.5019893646240234, depth: 0.0, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 1.1527957916259766, height: 0.6805601119995117, depth: 0.0, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.7361202239990234, height: 0.8194856643676758, depth: 0.0, italic: 0.0 }), // 0x7B
        Some(TfmChar { width: 0.8055667877197266, height: 0.4305543899536133, depth: 0.19507789611816406, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.16394329071044922, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.16394329071044922, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 1.0833492279052734, height: 0.6805601119995117, depth: 0.16394329071044922, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `msbm10.tfm`.
#[rustfmt::skip]
pub static MSBM10: TfmFont = TfmFont {
    name: "msbm10",
    space: 0.3000030517578125,
    skew_char: None,
    params: &[0.25, 0.3000030517578125, 0.149993896484375, 0.100006103515625, 0.4629640579223633, 1.0, 0.100006103515625, 0.6765079498291016, 0.39373207092285156, 0.44373130798339844, 0.6859512329101562, 0.34484100341796875, 0.41289234161376953, 0.36289215087890625, 0.28888893127441406, 0.14999961853027344, 0.24721717834472656, 0.3861083984375, 0.05000019073486328, 2.3899993896484375, 1.010000228881836, 0.25],
    chars: &[
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.7777805328369141, height: 0.7057647705078125, depth: 0.2057647705078125, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.7777805328369141, height: 0.7057647705078125, depth: 0.2057647705078125, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.7777805328369141, height: 0.7057647705078125, depth: 0.2057647705078125, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.7777805328369141, height: 0.7057647705078125, depth: 0.2057647705078125, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.7777805328369141, height: 0.7411098480224609, depth: 0.23222064971923828, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.7777805328369141, height: 0.7411098480224609, depth: 0.23222064971923828, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.7777805328369141, height: 0.7411098480224609, depth: 0.23222064971923828, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.7777805328369141, height: 0.7411098480224609, depth: 0.23222064971923828, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.7777805328369141, height: 0.9195127487182617, depth: 0.4195127487182617, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.7777805328369141, height: 0.9195127487182617, depth: 0.4195127487182617, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.26166820526123047, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.26166820526123047, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.26166820526123047, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.26166820526123047, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.7777805328369141, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 0.8888912200927734, height: 0.6888885498046875, depth: 0.1944427490234375, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 0.8888912200927734, height: 0.7411098480224609, depth: 0.1944427490234375, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.28481483459472656, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.28481483459472656, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.2514219284057617, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.7777805328369141, height: 0.6359710693359375, depth: 0.1359710693359375, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.5000019073486328, height: 0.7411098480224609, depth: 0.2514219284057617, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.27777862548828125, height: 0.7411098480224609, depth: 0.2514219284057617, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.22222328186035156, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.38889026641845703, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.6111125946044922, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.6111125946044922, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.7777805328369141, height: 0.7938289642333984, depth: 0.30274200439453125, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.7777805328369141, height: 0.7057647705078125, depth: 0.2057647705078125, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.7777805328369141, height: 0.7057647705078125, depth: 0.2057647705078125, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 1.0000028610229492, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.7777805328369141, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.7777805328369141, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.5555572509765625, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.6666688919067383, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6666688919067383, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6111125946044922, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.7777805328369141, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.7777805328369141, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.38889026641845703, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.5000019073486328, height: 0.6888885498046875, depth: 0.16666698455810547, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.7777805328369141, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.6666688919067383, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.9444475173950195, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.7777805328369141, height: 0.6888885498046875, depth: 0.16666698455810547, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.6111125946044922, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.7777805328369141, height: 0.6888885498046875, depth: 0.16666698455810547, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.5555572509765625, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.6666688919067383, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 1.0000028610229492, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.6666688919067383, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 1.888895034790039, height: 0.8250045776367188, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 2.333340644836426, height: 0.8250045776367188, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 1.888895034790039, height: 0.8250045776367188, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 2.333340644836426, height: 0.899998664855957, depth: 0.0, italic: 0.0 }), // 0x5E
        None, // 0x5F
        Some(TfmChar { width: 0.5555572509765625, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.638890266418457, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x61
        None, // 0x62
        None, // 0x63
        None, // 0x64
        None, // 0x65
        Some(TfmChar { width: 0.722224235534668, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.5555572509765625, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.7777805328369141, height: 0.4637479782104492, depth: -0.03625202178955078, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.6666688919067383, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.4444456100463867, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.6666688919067383, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.7777805328369141, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.7777805328369141, height: 0.539097785949707, depth: 0.03909778594970703, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.7777805328369141, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.7777805328369141, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.22222328186035156, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.38889026641845703, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.7777805328369141, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.7777805328369141, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.7777805328369141, height: 0.4831218719482422, depth: -0.016878128051757812, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.7777805328369141, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.26166820526123047, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.7777805328369141, height: 0.7572574615478516, depth: 0.26166820526123047, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 1.0000028610229492, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 1.0000028610229492, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.7777805328369141, height: 0.5816650390625, depth: 0.0816650390625, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.6666688919067383, height: 0.4305553436279297, depth: 0.0, italic: 0.04027843475341797 }), // 0x7B
        Some(TfmChar { width: 0.5555572509765625, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.5402803421020508, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.5402803421020508, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.429168701171875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `msbm7.tfm`.
#[rustfmt::skip]
pub static MSBM7: TfmFont = TfmFont {
    name: "msbm7",
    space: 0.3000030517578125,
    skew_char: None,
    params: &[0.25, 0.3000030517578125, 0.149993896484375, 0.100006103515625, 0.46295833587646484, 1.0, 0.100006103515625, 0.7323131561279297, 0.38390254974365234, 0.4712047576904297, 0.7521495819091797, 0.34421443939208984, 0.5025434494018555, 0.43111515045166016, 0.28571414947509766, 0.14285755157470703, 0.28571414947509766, 0.35317134857177734, 0.07142829895019531, 1.6999988555908203, 1.1571435928344727, 0.25],
    chars: &[
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.8928613662719727, height: 0.7550983428955078, depth: 0.2550983428955078, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.8928613662719727, height: 0.7550983428955078, depth: 0.2550983428955078, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.8928613662719727, height: 0.7550983428955078, depth: 0.2550983428955078, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.8928613662719727, height: 0.7550983428955078, depth: 0.2550983428955078, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 0.8928613662719727, height: 0.7787237167358398, depth: 0.27872371673583984, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 0.8928613662719727, height: 0.7929315567016602, depth: 0.29293155670166016, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 0.8928613662719727, height: 0.7787237167358398, depth: 0.27872371673583984, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.8928613662719727, height: 0.7929315567016602, depth: 0.29293155670166016, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 0.8928613662719727, height: 1.0040693283081055, depth: 0.5040693283081055, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 0.8928613662719727, height: 1.0040693283081055, depth: 0.5040693283081055, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.32991886138916016, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.32991886138916016, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.32991886138916016, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.32991886138916016, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 0.8928613662719727, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.0158777236938477, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 1.0158777236938477, height: 0.7550983428955078, depth: 0.1899566650390625, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 0.8928613662719727, height: 0.855778694152832, depth: 0.35577869415283203, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.8928613662719727, height: 0.855778694152832, depth: 0.35577869415283203, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.31954479217529297, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.8928613662719727, height: 0.6888885498046875, depth: 0.1899566650390625, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.5853204727172852, height: 0.7550983428955078, depth: 0.2550983428955078, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.33928775787353516, height: 0.7550983428955078, depth: 0.2550983428955078, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.27777957916259766, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.46230411529541016, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.7083368301391602, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.8313531875610352, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.7083368301391602, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.8313531875610352, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.8928613662719727, height: 0.8719873428344727, depth: 0.37198734283447266, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.8928613662719727, height: 0.7550983428955078, depth: 0.2550983428955078, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.8928613662719727, height: 0.7550983428955078, depth: 0.2550983428955078, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 1.1388940811157227, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 0.8928613662719727, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.8928613662719727, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.6468286514282227, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.666661262512207, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.666661262512207, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6111059188842773, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.77777099609375, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.77777099609375, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.388885498046875, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.49999523162841797, height: 0.6888885498046875, depth: 0.16666507720947266, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.77777099609375, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.666661262512207, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.9444360733032227, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.77777099609375, height: 0.6888885498046875, depth: 0.16666507720947266, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.6111059188842773, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.77777099609375, height: 0.6888885498046875, depth: 0.16666507720947266, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.5555505752563477, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.666661262512207, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 0.9999914169311523, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.7222156524658203, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.666661262512207, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 2.1230249404907227, height: 0.8247308731079102, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 2.6150903701782227, height: 0.8247308731079102, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 2.1230249404907227, height: 0.8247308731079102, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 2.6150903701782227, height: 0.899998664855957, depth: 0.0, italic: 0.0 }), // 0x5E
        None, // 0x5F
        Some(TfmChar { width: 0.6468286514282227, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.7390899658203125, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x61
        None, // 0x62
        None, // 0x63
        None, // 0x64
        None, // 0x65
        Some(TfmChar { width: 0.8313531875610352, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.6468286514282227, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.8928613662719727, height: 0.49387073516845703, depth: -0.006129264831542969, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.7698450088500977, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.5238122940063477, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.7698450088500977, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.8928613662719727, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.8928613662719727, height: 0.5756750106811523, depth: 0.07567501068115234, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.8928613662719727, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.8928613662719727, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.27777957916259766, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.46230411529541016, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.8928613662719727, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.8928613662719727, height: 0.3840789794921875, depth: -0.1159210205078125, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.8928613662719727, height: 0.5158281326293945, depth: 0.01582813262939453, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.8928613662719727, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.32991886138916016, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.8928613662719727, height: 0.8247308731079102, depth: 0.32991886138916016, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 1.1388940811157227, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 1.1388940811157227, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.8928613662719727, height: 0.6223344802856445, depth: 0.12233448028564453, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.7698450088500977, height: 0.4305553436279297, depth: 0.0, italic: 0.04762077331542969 }), // 0x7B
        Some(TfmChar { width: 0.5555505752563477, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.6329412460327148, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.6329412460327148, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.5069475173950195, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `msbm5.tfm`.
#[rustfmt::skip]
pub static MSBM5: TfmFont = TfmFont {
    name: "msbm5",
    space: 0.3000030517578125,
    skew_char: None,
    params: &[0.25, 0.3000030517578125, 0.149993896484375, 0.100006103515625, 0.46295166015625, 1.0, 0.100006103515625, 0.9252414703369141, 0.3874635696411133, 0.5041322708129883, 1.0252408981323242, 0.5319089889526367, 0.5035552978515625, 0.40355491638183594, 0.2944459915161133, 0.19999980926513672, 0.39999961853027344, 0.49444580078125, 0.10000038146972656, 1.9799985885620117, 1.4200010299682617, 0.25],
    chars: &[
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 1.0833492279052734, height: 0.8094882965087891, depth: 0.30948829650878906, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 1.0833492279052734, height: 0.8094882965087891, depth: 0.30948829650878906, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 1.0833492279052734, height: 0.8094882965087891, depth: 0.30948829650878906, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 1.0833492279052734, height: 0.8094882965087891, depth: 0.30948829650878906, italic: 0.0 }), // 0x07
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x08
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x09
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x0A
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x0B
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x0C
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x0D
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x0E
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x0F
        Some(TfmChar { width: 1.0833492279052734, height: 0.8094882965087891, depth: 0.30948829650878906, italic: 0.0 }), // 0x10
        Some(TfmChar { width: 1.0833492279052734, height: 0.8292055130004883, depth: 0.333404541015625, italic: 0.0 }), // 0x11
        Some(TfmChar { width: 1.0833492279052734, height: 0.8094882965087891, depth: 0.30948829650878906, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 1.0833492279052734, height: 0.8292055130004883, depth: 0.333404541015625, italic: 0.0 }), // 0x13
        Some(TfmChar { width: 1.0833492279052734, height: 1.0740604400634766, depth: 0.5740604400634766, italic: 0.0 }), // 0x14
        Some(TfmChar { width: 1.0833492279052734, height: 1.0740604400634766, depth: 0.5740604400634766, italic: 0.0 }), // 0x15
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x16
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.37713050842285156, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.37713050842285156, italic: 0.0 }), // 0x19
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.37713050842285156, italic: 0.0 }), // 0x1A
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.37713050842285156, italic: 0.0 }), // 0x1B
        Some(TfmChar { width: 1.0833492279052734, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x1C
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x1D
        Some(TfmChar { width: 1.2222414016723633, height: 0.6888885498046875, depth: 0.19444561004638672, italic: 0.0 }), // 0x1E
        Some(TfmChar { width: 1.2222414016723633, height: 0.75, depth: 0.19444561004638672, italic: 0.0 }), // 0x1F
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x21
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x22
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x23
        Some(TfmChar { width: 1.0833492279052734, height: 0.9023952484130859, depth: 0.4047975540161133, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 1.0833492279052734, height: 0.9023952484130859, depth: 0.4047975540161133, italic: 0.0 }), // 0x25
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.36572265625, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 1.0833492279052734, height: 0.7193202972412109, depth: 0.21932029724121094, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.7361202239990234, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.45833778381347656, height: 0.75, depth: 0.25, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.38889122009277344, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.59722900390625, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.8750123977661133, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 1.0139036178588867, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.8750123977661133, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 1.0139036178588867, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 1.0833492279052734, height: 0.9333248138427734, depth: 0.43332481384277344, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 1.0833492279052734, height: 0.8094882965087891, depth: 0.30948829650878906, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 1.0833492279052734, height: 0.8094882965087891, depth: 0.30948829650878906, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x3B
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x3C
        Some(TfmChar { width: 1.3611326217651367, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x3D
        Some(TfmChar { width: 1.0833492279052734, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 1.0833492279052734, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x3F
        Some(TfmChar { width: 0.8055667877197266, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x40
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.6666507720947266, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6666507720947266, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6110963821411133, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.7777585983276367, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.7777585983276367, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.38887977600097656, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.4999876022338867, height: 0.6888885498046875, depth: 0.16178321838378906, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.7777585983276367, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.6666507720947266, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.9444217681884766, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.7777585983276367, height: 0.6888885498046875, depth: 0.16178321838378906, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.6110963821411133, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.7777585983276367, height: 0.6888885498046875, depth: 0.16178321838378906, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.5555419921875, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.6666507720947266, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 0.9999752044677734, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.7222042083740234, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.6666507720947266, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 2.4722652435302734, height: 0.8292055130004883, depth: 0.0, italic: 0.0 }), // 0x5B
        Some(TfmChar { width: 3.02783203125, height: 0.8292055130004883, depth: 0.0, italic: 0.0 }), // 0x5C
        Some(TfmChar { width: 2.4722652435302734, height: 0.8292055130004883, depth: 0.0, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 3.02783203125, height: 0.9023952484130859, depth: 0.0, italic: 0.0 }), // 0x5E
        None, // 0x5F
        Some(TfmChar { width: 0.8055667877197266, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x60
        Some(TfmChar { width: 0.9097347259521484, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x61
        None, // 0x62
        None, // 0x63
        None, // 0x64
        None, // 0x65
        Some(TfmChar { width: 1.0139036178588867, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.8055667877197266, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 1.0833492279052734, height: 0.5148067474365234, depth: 0.014806747436523438, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.9444580078125, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.6666746139526367, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.9444580078125, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 1.0833492279052734, height: 0.6009159088134766, depth: 0.10091590881347656, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 1.0833492279052734, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 1.0833492279052734, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.38889122009277344, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.59722900390625, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 1.0833492279052734, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 1.0833492279052734, height: 0.3964052200317383, depth: -0.10359477996826172, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 1.0833492279052734, height: 0.5384883880615234, depth: 0.03848838806152344, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 1.0833492279052734, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.37713050842285156, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 1.0833492279052734, height: 0.8714265823364258, depth: 0.37713050842285156, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 1.3611326217651367, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 1.3611326217651367, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 1.0833492279052734, height: 0.6617832183837891, depth: 0.16178321838378906, italic: 0.0 }), // 0x7A
        Some(TfmChar { width: 0.9444580078125, height: 0.4305543899536133, depth: 0.0, italic: 0.0555572509765625 }), // 0x7B
        Some(TfmChar { width: 0.5555419921875, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7C
        Some(TfmChar { width: 0.7916774749755859, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7D
        Some(TfmChar { width: 0.7916774749755859, height: 0.6888885498046875, depth: 0.0, italic: 0.0 }), // 0x7E
        Some(TfmChar { width: 0.6458406448364258, height: 0.4305543899536133, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `cmti10.tfm`.
#[rustfmt::skip]
pub static CMTI10: TfmFont = TfmFont {
    name: "cmti10",
    space: 0.3577756881713867,
    skew_char: None,
    params: &[0.25, 0.3577756881713867, 0.15333271026611328, 0.10222148895263672, 0.4305553436279297, 1.022216796875, 0.10222148895263672],
    chars: &[
        Some(TfmChar { width: 0.6272182464599609, height: 0.6833324432373047, depth: 0.0, italic: 0.1330547332763672 }), // 0x00
        Some(TfmChar { width: 0.8177738189697266, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.76666259765625, height: 0.6833324432373047, depth: 0.0, italic: 0.09402751922607422 }), // 0x02
        Some(TfmChar { width: 0.6922178268432617, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.6644411087036133, height: 0.6833324432373047, depth: 0.0, italic: 0.15294361114501953 }), // 0x04
        Some(TfmChar { width: 0.7433290481567383, height: 0.6833324432373047, depth: 0.0, italic: 0.16388988494873047 }), // 0x05
        Some(TfmChar { width: 0.7155513763427734, height: 0.6833324432373047, depth: 0.0, italic: 0.12027740478515625 }), // 0x06
        Some(TfmChar { width: 0.76666259765625, height: 0.6833324432373047, depth: 0.0, italic: 0.11111164093017578 }), // 0x07
        Some(TfmChar { width: 0.7155513763427734, height: 0.6833324432373047, depth: 0.0, italic: 0.05986213684082031 }), // 0x08
        Some(TfmChar { width: 0.76666259765625, height: 0.6833324432373047, depth: 0.0, italic: 0.11111164093017578 }), // 0x09
        Some(TfmChar { width: 0.7155513763427734, height: 0.6833324432373047, depth: 0.0, italic: 0.102569580078125 }), // 0x0A
        Some(TfmChar { width: 0.6133298873901367, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.211944580078125 }), // 0x0B
        Some(TfmChar { width: 0.5622196197509766, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.10333442687988281 }), // 0x0C
        Some(TfmChar { width: 0.5877742767333984, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.10333442687988281 }), // 0x0D
        Some(TfmChar { width: 0.8816623687744141, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.10333442687988281 }), // 0x0E
        Some(TfmChar { width: 0.894439697265625, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.10333442687988281 }), // 0x0F
        Some(TfmChar { width: 0.30666542053222656, height: 0.4305553436279297, depth: 0.0, italic: 0.07671356201171875 }), // 0x10
        Some(TfmChar { width: 0.33222007751464844, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.03736114501953125 }), // 0x11
        Some(TfmChar { width: 0.5111083984375, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.5111083984375, height: 0.6944446563720703, depth: 0.0, italic: 0.09694480895996094 }), // 0x13
        Some(TfmChar { width: 0.5111083984375, height: 0.6284732818603516, depth: 0.0, italic: 0.08295249938964844 }), // 0x14
        Some(TfmChar { width: 0.5111083984375, height: 0.6944446563720703, depth: 0.0, italic: 0.10805702209472656 }), // 0x15
        Some(TfmChar { width: 0.5111083984375, height: 0.5616655349731445, depth: 0.0, italic: 0.10333442687988281 }), // 0x16
        Some(TfmChar { width: 0.8312911987304688, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x17
        Some(TfmChar { width: 0.45999717712402344, height: 0.0, depth: 0.17013835906982422, italic: 0.0 }), // 0x18
        Some(TfmChar { width: 0.5366640090942383, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.10513782501220703 }), // 0x19
        Some(TfmChar { width: 0.7155513763427734, height: 0.4305553436279297, depth: 0.0, italic: 0.07513904571533203 }), // 0x1A
        Some(TfmChar { width: 0.7155513763427734, height: 0.4305553436279297, depth: 0.0, italic: 0.07513904571533203 }), // 0x1B
        Some(TfmChar { width: 0.5111083984375, height: 0.5277786254882812, depth: 0.09722328186035156, italic: 0.09194469451904297 }), // 0x1C
        Some(TfmChar { width: 0.8827724456787109, height: 0.6833324432373047, depth: 0.0, italic: 0.12027740478515625 }), // 0x1D
        Some(TfmChar { width: 0.9849948883056641, height: 0.6833324432373047, depth: 0.0, italic: 0.12027740478515625 }), // 0x1E
        Some(TfmChar { width: 0.76666259765625, height: 0.7319440841674805, depth: 0.04861164093017578, italic: 0.09402751922607422 }), // 0x1F
        Some(TfmChar { width: 0.25555419921875, height: 0.4305553436279297, depth: 0.0, italic: 0.0 }), // 0x20
        Some(TfmChar { width: 0.30666542053222656, height: 0.6944446563720703, depth: 0.0, italic: 0.12416839599609375 }), // 0x21
        Some(TfmChar { width: 0.5144424438476562, height: 0.6944446563720703, depth: 0.0, italic: 0.06961250305175781 }), // 0x22
        Some(TfmChar { width: 0.8177738189697266, height: 0.6944446563720703, depth: 0.1944427490234375, italic: 0.06616401672363281 }), // 0x23
        Some(TfmChar { width: 0.7690935134887695, height: 0.6944446563720703, depth: 0.0, italic: 0.0 }), // 0x24
        Some(TfmChar { width: 0.8177738189697266, height: 0.75, depth: 0.05555534362792969, italic: 0.13638877868652344 }), // 0x25
        Some(TfmChar { width: 0.76666259765625, height: 0.6944446563720703, depth: 0.0, italic: 0.09694480895996094 }), // 0x26
        Some(TfmChar { width: 0.30666542053222656, height: 0.6944446563720703, depth: 0.0, italic: 0.12416839599609375 }), // 0x27
        Some(TfmChar { width: 0.4088869094848633, height: 0.75, depth: 0.25, italic: 0.16194438934326172 }), // 0x28
        Some(TfmChar { width: 0.4088869094848633, height: 0.75, depth: 0.25, italic: 0.03694438934326172 }), // 0x29
        Some(TfmChar { width: 0.5111083984375, height: 0.75, depth: 0.0, italic: 0.14916706085205078 }), // 0x2A
        Some(TfmChar { width: 0.76666259765625, height: 0.5616655349731445, depth: 0.05666542053222656, italic: 0.03694438934326172 }), // 0x2B
        Some(TfmChar { width: 0.30666542053222656, height: 0.10555553436279297, depth: 0.1944446563720703, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.3577756881713867, height: 0.4305553436279297, depth: 0.0, italic: 0.02826404571533203 }), // 0x2D
        Some(TfmChar { width: 0.30666542053222656, height: 0.10555553436279297, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.5111083984375, height: 0.75, depth: 0.25, italic: 0.16194438934326172 }), // 0x2F
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.0, italic: 0.13555622100830078 }), // 0x30
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.0, italic: 0.13555622100830078 }), // 0x31
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.0, italic: 0.13555622100830078 }), // 0x32
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.0, italic: 0.13555622100830078 }), // 0x33
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.1944446563720703, italic: 0.13555622100830078 }), // 0x34
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.0, italic: 0.13555622100830078 }), // 0x35
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.0, italic: 0.13555622100830078 }), // 0x36
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.1944446563720703, italic: 0.13555622100830078 }), // 0x37
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.0, italic: 0.13555622100830078 }), // 0x38
        Some(TfmChar { width: 0.5111083984375, height: 0.644444465637207, depth: 0.0, italic: 0.13555622100830078 }), // 0x39
        Some(TfmChar { width: 0.30666542053222656, height: 0.4305553436279297, depth: 0.0, italic: 0.05819511413574219 }), // 0x3A
        Some(TfmChar { width: 0.30666542053222656, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.05819511413574219 }), // 0x3B
        Some(TfmChar { width: 0.30666542053222656, height: 0.5, depth: 0.1944446563720703, italic: 0.07555675506591797 }), // 0x3C
        Some(TfmChar { width: 0.76666259765625, height: 0.36687469482421875, depth: -0.13312530517578125, italic: 0.06616401672363281 }), // 0x3D
        Some(TfmChar { width: 0.5111083984375, height: 0.5, depth: 0.1944446563720703, italic: 0.0 }), // 0x3E
        Some(TfmChar { width: 0.5111083984375, height: 0.6944446563720703, depth: 0.0, italic: 0.12250041961669922 }), // 0x3F
        Some(TfmChar { width: 0.76666259765625, height: 0.6944446563720703, depth: 0.0, italic: 0.09597206115722656 }), // 0x40
        Some(TfmChar { width: 0.7433290481567383, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.7038850784301758, height: 0.6833324432373047, depth: 0.0, italic: 0.102569580078125 }), // 0x42
        Some(TfmChar { width: 0.7155513763427734, height: 0.6833324432373047, depth: 0.0, italic: 0.1452770233154297 }), // 0x43
        Some(TfmChar { width: 0.7549953460693359, height: 0.6833324432373047, depth: 0.0, italic: 0.09402751922607422 }), // 0x44
        Some(TfmChar { width: 0.6783294677734375, height: 0.6833324432373047, depth: 0.0, italic: 0.12027740478515625 }), // 0x45
        Some(TfmChar { width: 0.6527738571166992, height: 0.6833324432373047, depth: 0.0, italic: 0.1330547332763672 }), // 0x46
        Some(TfmChar { width: 0.7736072540283203, height: 0.6833324432373047, depth: 0.0, italic: 0.08722209930419922 }), // 0x47
        Some(TfmChar { width: 0.7433290481567383, height: 0.6833324432373047, depth: 0.0, italic: 0.16388988494873047 }), // 0x48
        Some(TfmChar { width: 0.38555335998535156, height: 0.6833324432373047, depth: 0.0, italic: 0.15805530548095703 }), // 0x49
        Some(TfmChar { width: 0.5249967575073242, height: 0.6833324432373047, depth: 0.0, italic: 0.14027881622314453 }), // 0x4A
        Some(TfmChar { width: 0.7688846588134766, height: 0.6833324432373047, depth: 0.0, italic: 0.1452770233154297 }), // 0x4B
        Some(TfmChar { width: 0.6272182464599609, height: 0.6833324432373047, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 0.8966617584228516, height: 0.6833324432373047, depth: 0.0, italic: 0.16388988494873047 }), // 0x4D
        Some(TfmChar { width: 0.7433290481567383, height: 0.6833324432373047, depth: 0.0, italic: 0.16388988494873047 }), // 0x4E
        Some(TfmChar { width: 0.76666259765625, height: 0.6833324432373047, depth: 0.0, italic: 0.09402751922607422 }), // 0x4F
        Some(TfmChar { width: 0.6783294677734375, height: 0.6833324432373047, depth: 0.0, italic: 0.102569580078125 }), // 0x50
        Some(TfmChar { width: 0.76666259765625, height: 0.6833324432373047, depth: 0.1944446563720703, italic: 0.09402751922607422 }), // 0x51
        Some(TfmChar { width: 0.7294406890869141, height: 0.6833324432373047, depth: 0.0, italic: 0.0386810302734375 }), // 0x52
        Some(TfmChar { width: 0.5622196197509766, height: 0.6833324432373047, depth: 0.0, italic: 0.11972236633300781 }), // 0x53
        Some(TfmChar { width: 0.7155513763427734, height: 0.6833324432373047, depth: 0.0, italic: 0.1330547332763672 }), // 0x54
        Some(TfmChar { width: 0.7433290481567383, height: 0.6833324432373047, depth: 0.0, italic: 0.16388988494873047 }), // 0x55
        Some(TfmChar { width: 0.7433290481567383, height: 0.6833324432373047, depth: 0.0, italic: 0.1836109161376953 }), // 0x56
        Some(TfmChar { width: 0.9988832473754883, height: 0.6833324432373047, depth: 0.0, italic: 0.1836109161376953 }), // 0x57
        Some(TfmChar { width: 0.7433290481567383, height: 0.6833324432373047, depth: 0.0, italic: 0.15805530548095703 }), // 0x58
        Some(TfmChar { width: 0.7433290481567383, height: 0.6833324432373047, depth: 0.0, italic: 0.1938323974609375 }), // 0x59
        Some(TfmChar { width: 0.6133298873901367, height: 0.6833324432373047, depth: 0.0, italic: 0.1452770233154297 }), // 0x5A
        Some(TfmChar { width: 0.30666542053222656, height: 0.75, depth: 0.25, italic: 0.1875 }), // 0x5B
        Some(TfmChar { width: 0.5144424438476562, height: 0.6944446563720703, depth: 0.0, italic: 0.16849994659423828 }), // 0x5C
        Some(TfmChar { width: 0.30666542053222656, height: 0.75, depth: 0.25, italic: 0.10527801513671875 }), // 0x5D
        Some(TfmChar { width: 0.5111083984375, height: 0.6944446563720703, depth: 0.0, italic: 0.06645965576171875 }), // 0x5E
        Some(TfmChar { width: 0.30666542053222656, height: 0.6678590774536133, depth: 0.0, italic: 0.11752128601074219 }), // 0x5F
        Some(TfmChar { width: 0.30666542053222656, height: 0.6944446563720703, depth: 0.0, italic: 0.12416839599609375 }), // 0x60
        Some(TfmChar { width: 0.5111083984375, height: 0.4305553436279297, depth: 0.0, italic: 0.07671356201171875 }), // 0x61
        Some(TfmChar { width: 0.45999717712402344, height: 0.6944446563720703, depth: 0.0, italic: 0.06312370300292969 }), // 0x62
        Some(TfmChar { width: 0.45999717712402344, height: 0.4305553436279297, depth: 0.0, italic: 0.05652809143066406 }), // 0x63
        Some(TfmChar { width: 0.5111083984375, height: 0.6944446563720703, depth: 0.0, italic: 0.10333442687988281 }), // 0x64
        Some(TfmChar { width: 0.45999717712402344, height: 0.4305553436279297, depth: 0.0, italic: 0.07513904571533203 }), // 0x65
        Some(TfmChar { width: 0.30666542053222656, height: 0.6944446563720703, depth: 0.1944446563720703, italic: 0.211944580078125 }), // 0x66
        Some(TfmChar { width: 0.45999717712402344, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.08847236633300781 }), // 0x67
        Some(TfmChar { width: 0.5111083984375, height: 0.6944446563720703, depth: 0.0, italic: 0.07671356201171875 }), // 0x68
        Some(TfmChar { width: 0.30666542053222656, height: 0.6553592681884766, depth: 0.0, italic: 0.10189628601074219 }), // 0x69
        Some(TfmChar { width: 0.30666542053222656, height: 0.6553592681884766, depth: 0.1944446563720703, italic: 0.14467334747314453 }), // 0x6A
        Some(TfmChar { width: 0.45999717712402344, height: 0.6944446563720703, depth: 0.0, italic: 0.10763835906982422 }), // 0x6B
        Some(TfmChar { width: 0.25555419921875, height: 0.6944446563720703, depth: 0.0, italic: 0.10333442687988281 }), // 0x6C
        Some(TfmChar { width: 0.8177738189697266, height: 0.4305553436279297, depth: 0.0, italic: 0.07671356201171875 }), // 0x6D
        Some(TfmChar { width: 0.5622196197509766, height: 0.4305553436279297, depth: 0.0, italic: 0.07671356201171875 }), // 0x6E
        Some(TfmChar { width: 0.5111083984375, height: 0.4305553436279297, depth: 0.0, italic: 0.06312370300292969 }), // 0x6F
        Some(TfmChar { width: 0.5111083984375, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.06312370300292969 }), // 0x70
        Some(TfmChar { width: 0.45999717712402344, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.08847236633300781 }), // 0x71
        Some(TfmChar { width: 0.4216642379760742, height: 0.4305553436279297, depth: 0.0, italic: 0.10763835906982422 }), // 0x72
        Some(TfmChar { width: 0.4088869094848633, height: 0.4305553436279297, depth: 0.0, italic: 0.08208274841308594 }), // 0x73
        Some(TfmChar { width: 0.33222007751464844, height: 0.6150798797607422, depth: 0.0, italic: 0.09486103057861328 }), // 0x74
        Some(TfmChar { width: 0.5366640090942383, height: 0.4305553436279297, depth: 0.0, italic: 0.07671356201171875 }), // 0x75
        Some(TfmChar { width: 0.45999717712402344, height: 0.4305553436279297, depth: 0.0, italic: 0.10763835906982422 }), // 0x76
        Some(TfmChar { width: 0.6644411087036133, height: 0.4305553436279297, depth: 0.0, italic: 0.10763835906982422 }), // 0x77
        Some(TfmChar { width: 0.46388721466064453, height: 0.4305553436279297, depth: 0.0, italic: 0.12041664123535156 }), // 0x78
        Some(TfmChar { width: 0.4855527877807617, height: 0.4305553436279297, depth: 0.1944446563720703, italic: 0.08847236633300781 }), // 0x79
        Some(TfmChar { width: 0.4088869094848633, height: 0.4305553436279297, depth: 0.0, italic: 0.12291717529296875 }), // 0x7A
        Some(TfmChar { width: 0.5111083984375, height: 0.4305553436279297, depth: 0.0, italic: 0.09207916259765625 }), // 0x7B
        Some(TfmChar { width: 1.022216796875, height: 0.4305553436279297, depth: 0.0, italic: 0.09207916259765625 }), // 0x7C
        Some(TfmChar { width: 0.5111083984375, height: 0.6944446563720703, depth: 0.0, italic: 0.12250041961669922 }), // 0x7D
        Some(TfmChar { width: 0.5111083984375, height: 0.6678590774536133, depth: 0.0, italic: 0.11585426330566406 }), // 0x7E
        Some(TfmChar { width: 0.5111083984375, height: 0.6678590774536133, depth: 0.0, italic: 0.10474395751953125 }), // 0x7F
    ],
    kerns: &[
        (0x0B, 0x21, 0.10430622100830078),
        (0x0B, 0x27, 0.10430622100830078),
        (0x0B, 0x29, 0.10430622100830078),
        (0x0B, 0x3F, 0.10430622100830078),
        (0x0B, 0x5D, 0.10430622100830078),
        (0x20, 0x4C, -0.3205537796020508),
        (0x20, 0x6C, -0.25555419921875),
        (0x27, 0x21, 0.10222148895263672),
        (0x27, 0x3F, 0.10222148895263672),
        (0x41, 0x43, -0.02555561065673828),
        (0x41, 0x47, -0.02555561065673828),
        (0x41, 0x4F, -0.02555561065673828),
        (0x41, 0x51, -0.02555561065673828),
        (0x41, 0x54, -0.07666587829589844),
        (0x41, 0x55, -0.02555561065673828),
        (0x41, 0x56, -0.10222148895263672),
        (0x41, 0x57, -0.10222148895263672),
        (0x41, 0x59, -0.07666587829589844),
        (0x41, 0x61, -0.05111122131347656),
        (0x41, 0x62, -0.02555561065673828),
        (0x41, 0x63, -0.05111122131347656),
        (0x41, 0x64, -0.05111122131347656),
        (0x41, 0x65, -0.05111122131347656),
        (0x41, 0x67, -0.05111122131347656),
        (0x41, 0x68, -0.02555561065673828),
        (0x41, 0x69, -0.02555561065673828),
        (0x41, 0x6B, -0.02555561065673828),
        (0x41, 0x6C, -0.02555561065673828),
        (0x41, 0x6D, -0.02555561065673828),
        (0x41, 0x6E, -0.02555561065673828),
        (0x41, 0x6F, -0.05111122131347656),
        (0x41, 0x71, -0.05111122131347656),
        (0x41, 0x72, -0.02555561065673828),
        (0x41, 0x74, -0.02555561065673828),
        (0x41, 0x75, -0.02555561065673828),
        (0x41, 0x76, -0.02555561065673828),
        (0x41, 0x77, -0.02555561065673828),
        (0x44, 0x41, -0.02555561065673828),
        (0x44, 0x56, -0.02555561065673828),
        (0x44, 0x57, -0.02555561065673828),
        (0x44, 0x58, -0.02555561065673828),
        (0x44, 0x59, -0.02555561065673828),
        (0x46, 0x41, -0.10222148895263672),
        (0x46, 0x43, -0.02555561065673828),
        (0x46, 0x47, -0.02555561065673828),
        (0x46, 0x4F, -0.02555561065673828),
        (0x46, 0x51, -0.02555561065673828),
        (0x46, 0x61, -0.07666587829589844),
        (0x46, 0x65, -0.07666587829589844),
        (0x46, 0x6F, -0.07666587829589844),
        (0x46, 0x72, -0.07666587829589844),
        (0x46, 0x75, -0.07666587829589844),
        (0x4B, 0x43, -0.02555561065673828),
        (0x4B, 0x47, -0.02555561065673828),
        (0x4B, 0x4F, -0.02555561065673828),
        (0x4B, 0x51, -0.02555561065673828),
        (0x4C, 0x54, -0.07666587829589844),
        (0x4C, 0x56, -0.10222148895263672),
        (0x4C, 0x57, -0.10222148895263672),
        (0x4C, 0x59, -0.07666587829589844),
        (0x4C, 0x61, -0.05111122131347656),
        (0x4C, 0x63, -0.05111122131347656),
        (0x4C, 0x64, -0.05111122131347656),
        (0x4C, 0x65, -0.05111122131347656),
        (0x4C, 0x67, -0.05111122131347656),
        (0x4C, 0x6F, -0.05111122131347656),
        (0x4C, 0x71, -0.05111122131347656),
        (0x4F, 0x41, -0.02555561065673828),
        (0x4F, 0x56, -0.02555561065673828),
        (0x4F, 0x57, -0.02555561065673828),
        (0x4F, 0x58, -0.02555561065673828),
        (0x4F, 0x59, -0.02555561065673828),
        (0x50, 0x41, -0.07666587829589844),
        (0x52, 0x43, -0.02555561065673828),
        (0x52, 0x47, -0.02555561065673828),
        (0x52, 0x4F, -0.02555561065673828),
        (0x52, 0x51, -0.02555561065673828),
        (0x52, 0x54, -0.07666587829589844),
        (0x52, 0x55, -0.02555561065673828),
        (0x52, 0x56, -0.10222148895263672),
        (0x52, 0x57, -0.10222148895263672),
        (0x52, 0x59, -0.07666587829589844),
        (0x52, 0x61, -0.05111122131347656),
        (0x52, 0x62, -0.02555561065673828),
        (0x52, 0x63, -0.05111122131347656),
        (0x52, 0x64, -0.05111122131347656),
        (0x52, 0x65, -0.05111122131347656),
        (0x52, 0x67, -0.05111122131347656),
        (0x52, 0x68, -0.02555561065673828),
        (0x52, 0x69, -0.02555561065673828),
        (0x52, 0x6B, -0.02555561065673828),
        (0x52, 0x6C, -0.02555561065673828),
        (0x52, 0x6D, -0.02555561065673828),
        (0x52, 0x6E, -0.02555561065673828),
        (0x52, 0x6F, -0.05111122131347656),
        (0x52, 0x71, -0.05111122131347656),
        (0x52, 0x72, -0.02555561065673828),
        (0x52, 0x74, -0.02555561065673828),
        (0x52, 0x75, -0.02555561065673828),
        (0x52, 0x76, -0.02555561065673828),
        (0x52, 0x77, -0.02555561065673828),
        (0x54, 0x41, -0.07666587829589844),
        (0x54, 0x61, -0.07666587829589844),
        (0x54, 0x65, -0.07666587829589844),
        (0x54, 0x6F, -0.07666587829589844),
        (0x54, 0x72, -0.07666587829589844),
        (0x54, 0x75, -0.07666587829589844),
        (0x54, 0x79, -0.07666587829589844),
        (0x56, 0x41, -0.10222148895263672),
        (0x56, 0x43, -0.02555561065673828),
        (0x56, 0x47, -0.02555561065673828),
        (0x56, 0x4F, -0.02555561065673828),
        (0x56, 0x51, -0.02555561065673828),
        (0x56, 0x61, -0.07666587829589844),
        (0x56, 0x65, -0.07666587829589844),
        (0x56, 0x6F, -0.07666587829589844),
        (0x56, 0x72, -0.07666587829589844),
        (0x56, 0x75, -0.07666587829589844),
        (0x57, 0x41, -0.07666587829589844),
        (0x58, 0x43, -0.02555561065673828),
        (0x58, 0x47, -0.02555561065673828),
        (0x58, 0x4F, -0.02555561065673828),
        (0x58, 0x51, -0.02555561065673828),
        (0x59, 0x41, -0.07666587829589844),
        (0x59, 0x61, -0.07666587829589844),
        (0x59, 0x65, -0.07666587829589844),
        (0x59, 0x6F, -0.07666587829589844),
        (0x59, 0x72, -0.07666587829589844),
        (0x59, 0x75, -0.07666587829589844),
        (0x62, 0x61, -0.05111122131347656),
        (0x62, 0x63, -0.05111122131347656),
        (0x62, 0x64, -0.05111122131347656),
        (0x62, 0x65, -0.05111122131347656),
        (0x62, 0x67, -0.05111122131347656),
        (0x62, 0x6F, -0.05111122131347656),
        (0x62, 0x71, -0.05111122131347656),
        (0x63, 0x61, -0.05111122131347656),
        (0x63, 0x63, -0.05111122131347656),
        (0x63, 0x64, -0.05111122131347656),
        (0x63, 0x65, -0.05111122131347656),
        (0x63, 0x67, -0.05111122131347656),
        (0x63, 0x6F, -0.05111122131347656),
        (0x63, 0x71, -0.05111122131347656),
        (0x64, 0x6C, 0.05111122131347656),
        (0x65, 0x61, -0.05111122131347656),
        (0x65, 0x63, -0.05111122131347656),
        (0x65, 0x64, -0.05111122131347656),
        (0x65, 0x65, -0.05111122131347656),
        (0x65, 0x67, -0.05111122131347656),
        (0x65, 0x6F, -0.05111122131347656),
        (0x65, 0x71, -0.05111122131347656),
        (0x66, 0x21, 0.10430622100830078),
        (0x66, 0x27, 0.10430622100830078),
        (0x66, 0x29, 0.10430622100830078),
        (0x66, 0x3F, 0.10430622100830078),
        (0x66, 0x5D, 0.10430622100830078),
        (0x6C, 0x6C, 0.05111122131347656),
        (0x6E, 0x27, -0.10222148895263672),
        (0x6F, 0x61, -0.05111122131347656),
        (0x6F, 0x63, -0.05111122131347656),
        (0x6F, 0x64, -0.05111122131347656),
        (0x6F, 0x65, -0.05111122131347656),
        (0x6F, 0x67, -0.05111122131347656),
        (0x6F, 0x6F, -0.05111122131347656),
        (0x6F, 0x71, -0.05111122131347656),
        (0x70, 0x61, -0.05111122131347656),
        (0x70, 0x63, -0.05111122131347656),
        (0x70, 0x64, -0.05111122131347656),
        (0x70, 0x65, -0.05111122131347656),
        (0x70, 0x67, -0.05111122131347656),
        (0x70, 0x6F, -0.05111122131347656),
        (0x70, 0x71, -0.05111122131347656),
        (0x72, 0x61, -0.05111122131347656),
        (0x72, 0x63, -0.05111122131347656),
        (0x72, 0x64, -0.05111122131347656),
        (0x72, 0x65, -0.05111122131347656),
        (0x72, 0x67, -0.05111122131347656),
        (0x72, 0x6F, -0.05111122131347656),
        (0x72, 0x71, -0.05111122131347656),
        (0x77, 0x6C, 0.05111122131347656),
    ],
};

/// `eufm10.tfm`.
#[rustfmt::skip]
pub static EUFM10: TfmFont = TfmFont {
    name: "eufm10",
    space: 0.33333301544189453,
    skew_char: None,
    params: &[0.0, 0.33333301544189453, 0.0, 0.0, 0.475341796875, 1.0, 0.0, 0.37811279296875, 0.27008056640625, 0.297088623046875, 0.37811279296875, 0.16204833984375, 0.405120849609375, 0.37811279296875, 0.3240966796875, 0.189056396484375, 0.243072509765625, 0.405120849609375, 0.027008056640625, 2.198452949523926, 0.999298095703125, 0.2565765380859375],
    chars: &[
        Some(TfmChar { width: 0.49748802185058594, height: 0.621185302734375, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.49829864501953125, height: 0.621185302734375, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.3332796096801758, height: 0.69140625, depth: 0.189056396484375, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.32922840118408203, height: 0.69140625, depth: 0.189056396484375, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.5034303665161133, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.3330097198486328, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.3340892791748047, height: 0.621185302734375, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.5007295608520508, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x07
        None, // 0x08
        None, // 0x09
        None, // 0x0A
        None, // 0x0B
        None, // 0x0C
        None, // 0x0D
        None, // 0x0E
        None, // 0x0F
        None, // 0x10
        None, // 0x11
        Some(TfmChar { width: 0.21471405029296875, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.21471405029296875, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x13
        None, // 0x14
        None, // 0x15
        None, // 0x16
        None, // 0x17
        None, // 0x18
        None, // 0x19
        None, // 0x1A
        None, // 0x1B
        None, // 0x1C
        None, // 0x1D
        None, // 0x1E
        None, // 0x1F
        None, // 0x20
        Some(TfmChar { width: 0.29573822021484375, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x21
        None, // 0x22
        None, // 0x23
        None, // 0x24
        None, // 0x25
        Some(TfmChar { width: 0.7378597259521484, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.21201324462890625, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.38864612579345703, height: 0.7494735717773438, depth: 0.24982452392578125, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.38864612579345703, height: 0.7494735717773438, depth: 0.24982452392578125, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.27764320373535156, height: 0.621185302734375, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.7562255859375, height: 0.5828342437744141, depth: 0.08318519592285156, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.27764320373535156, height: 0.1080322265625, depth: 0.0, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.7562255859375, height: 0.5828342437744141, depth: 0.08318519592285156, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.27764320373535156, height: 0.1080322265625, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.5018100738525391, height: 0.7494735717773438, depth: 0.24982452392578125, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.5018100738525391, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.5018100738525391, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.5018100738525391, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.5018100738525391, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.5018100738525391, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.5018100738525391, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.5018100738525391, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.5018100738525391, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.5018100738525391, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.5018100738525391, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.216064453125, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.216064453125, height: 0.475341796875, depth: 0.12603759765625, italic: 0.0 }), // 0x3B
        None, // 0x3C
        Some(TfmChar { width: 0.7562255859375, height: 0.36865997314453125, depth: -0.13098907470703125, italic: 0.0 }), // 0x3D
        None, // 0x3E
        Some(TfmChar { width: 0.36244773864746094, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x3F
        None, // 0x40
        Some(TfmChar { width: 0.7176036834716797, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 0.8839740753173828, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.6125431060791016, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.831578254699707, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.6627779006958008, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.6111927032470703, height: 0.69140625, depth: 0.12603759765625, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.7853946685791016, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.7203044891357422, height: 0.69140625, depth: 0.063018798828125, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.5544757843017578, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.5523147583007812, height: 0.69140625, depth: 0.12603759765625, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.6684494018554688, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.6660184860229492, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.0495328903198242, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.832118034362793, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.8269863128662109, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.8275270462036133, height: 0.69140625, depth: 0.189056396484375, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.8269863128662109, height: 0.69140625, depth: 0.037811279296875, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.8280668258666992, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.8286075592041016, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.6689891815185547, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.6457624435424805, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.8313083648681641, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 1.0460224151611328, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 0.7192249298095703, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.8329286575317383, height: 0.69140625, depth: 0.189056396484375, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.6020097732543945, height: 0.69140625, depth: 0.12603759765625, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.27764320373535156, height: 0.7494735717773438, depth: 0.24982452392578125, italic: 0.0 }), // 0x5B
        None, // 0x5C
        Some(TfmChar { width: 0.27764320373535156, height: 0.7494735717773438, depth: 0.24982452392578125, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.4996490478515625, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x5E
        None, // 0x5F
        None, // 0x60
        Some(TfmChar { width: 0.5004596710205078, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.513153076171875, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.38945579528808594, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.4985685348510742, height: 0.621185302734375, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.4005298614501953, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.32625770568847656, height: 0.69140625, depth: 0.189056396484375, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.5037002563476562, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.5212554931640625, height: 0.69140625, depth: 0.189056396484375, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.2789936065673828, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.2808837890625, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.38945579528808594, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.27953338623046875, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.766758918762207, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.5266571044921875, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.4888458251953125, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.5004596710205078, height: 0.523956298828125, depth: 0.189056396484375, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.48911571502685547, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.38918590545654297, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.44266223907470703, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.3330097198486328, height: 0.621185302734375, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.5172042846679688, height: 0.475341796875, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.5118026733398438, height: 0.523956298828125, depth: 0.0, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.7735109329223633, height: 0.523956298828125, depth: 0.0, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.38864612579345703, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.4988384246826172, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.3905363082885742, height: 0.475341796875, depth: 0.189056396484375, italic: 0.0 }), // 0x7A
        None, // 0x7B
        None, // 0x7C
        Some(TfmChar { width: 0.21471405029296875, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x7D
        None, // 0x7E
        Some(TfmChar { width: 0.37811279296875, height: 0.69140625, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `eufm7.tfm`.
#[rustfmt::skip]
pub static EUFM7: TfmFont = TfmFont {
    name: "eufm7",
    space: 0.33333301544189453,
    skew_char: None,
    params: &[0.0, 0.33333301544189453, 0.0, 0.0, 0.4757251739501953, 1.0, 0.0, 0.37841796875, 0.27029895782470703, 0.29732799530029297, 0.37841796875, 0.16217899322509766, 0.40544795989990234, 0.37841796875, 0.3243579864501953, 0.189208984375, 0.2432689666748047, 0.40544795989990234, 0.027029991149902344, 2.200227737426758, 1.0001049041748047, 0.25678348541259766],
    chars: &[
        Some(TfmChar { width: 0.596461296081543, height: 0.6216869354248047, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.5973377227783203, height: 0.6216869354248047, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.4188671112060547, height: 0.6919641494750977, depth: 0.189208984375, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.41448497772216797, height: 0.6919641494750977, depth: 0.189208984375, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.6028881072998047, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.41857433319091797, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.41974353790283203, height: 0.6216869354248047, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.5999670028686523, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x07
        None, // 0x08
        None, // 0x09
        None, // 0x0A
        None, // 0x0B
        None, // 0x0C
        None, // 0x0D
        None, // 0x0E
        None, // 0x0F
        None, // 0x10
        None, // 0x11
        Some(TfmChar { width: 0.2906360626220703, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.2906360626220703, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x13
        None, // 0x14
        None, // 0x15
        None, // 0x16
        None, // 0x17
        None, // 0x18
        None, // 0x19
        None, // 0x1A
        None, // 0x1B
        None, // 0x1C
        None, // 0x1D
        None, // 0x1E
        None, // 0x1F
        None, // 0x20
        Some(TfmChar { width: 0.378265380859375, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x21
        None, // 0x22
        None, // 0x23
        None, // 0x24
        None, // 0x25
        Some(TfmChar { width: 0.8564281463623047, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.28771495819091797, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.4787464141845703, height: 0.7500782012939453, depth: 0.25002574920654297, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.4787464141845703, height: 0.7500782012939453, depth: 0.25002574920654297, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.35869503021240234, height: 0.6216869354248047, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 0.8762903213500977, height: 0.5833044052124023, depth: 0.083251953125, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.35869503021240234, height: 0.10811901092529297, depth: 0.0, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 0.8762903213500977, height: 0.5833044052124023, depth: 0.083251953125, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.35869503021240234, height: 0.10811901092529297, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.60113525390625, height: 0.7500782012939453, depth: 0.25002574920654297, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.60113525390625, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.60113525390625, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.60113525390625, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.60113525390625, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.60113525390625, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.60113525390625, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.60113525390625, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.60113525390625, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.60113525390625, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.60113525390625, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.2920970916748047, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.2920970916748047, height: 0.4757251739501953, depth: 0.12613964080810547, italic: 0.0 }), // 0x3B
        None, // 0x3C
        Some(TfmChar { width: 0.8762903213500977, height: 0.36895751953125, depth: -0.13109493255615234, italic: 0.0 }), // 0x3D
        None, // 0x3E
        Some(TfmChar { width: 0.45041370391845703, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x3F
        None, // 0x40
        Some(TfmChar { width: 0.8345203399658203, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 1.0144519805908203, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.7208948135375977, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 0.9577856063842773, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.7752246856689453, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.7194347381591797, height: 0.6919641494750977, depth: 0.12613964080810547, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 0.9078369140625, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 0.8374414443969727, height: 0.6919641494750977, depth: 0.06306934356689453, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.6580944061279297, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.655756950378418, height: 0.6919641494750977, depth: 0.12613964080810547, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.7813587188720703, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.7787303924560547, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.193507194519043, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 0.958369255065918, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 0.95281982421875, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 0.953404426574707, height: 0.6919641494750977, depth: 0.189208984375, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 0.95281982421875, height: 0.6919641494750977, depth: 0.037841796875, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 0.9539880752563477, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 0.9545726776123047, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.7819433212280273, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.7568225860595703, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 0.957493782043457, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 1.1897106170654297, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 0.836273193359375, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 0.9592456817626953, height: 0.6919641494750977, depth: 0.189208984375, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.709503173828125, height: 0.6919641494750977, depth: 0.12613964080810547, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.35869503021240234, height: 0.7500782012939453, depth: 0.25002574920654297, italic: 0.0 }), // 0x5B
        None, // 0x5C
        Some(TfmChar { width: 0.35869503021240234, height: 0.7500782012939453, depth: 0.25002574920654297, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.5987987518310547, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x5E
        None, // 0x5F
        None, // 0x60
        Some(TfmChar { width: 0.599675178527832, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.6134033203125, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.47962284088134766, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.597630500793457, height: 0.6216869354248047, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.49159908294677734, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.4112720489501953, height: 0.6919641494750977, depth: 0.189208984375, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.603179931640625, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.622166633605957, height: 0.6919641494750977, depth: 0.189208984375, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.3601551055908203, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.3621997833251953, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.47962284088134766, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.36073970794677734, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 0.8876819610595703, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.6280078887939453, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.5871143341064453, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.599675178527832, height: 0.524378776550293, depth: 0.189208984375, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.587407112121582, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.47933101654052734, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.537165641784668, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.41857433319091797, height: 0.6216869354248047, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.6177845001220703, height: 0.4757251739501953, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.611943244934082, height: 0.524378776550293, depth: 0.0, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 0.894984245300293, height: 0.524378776550293, depth: 0.0, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.4787464141845703, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.5979223251342773, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.4807910919189453, height: 0.4757251739501953, depth: 0.189208984375, italic: 0.0 }), // 0x7A
        None, // 0x7B
        None, // 0x7C
        Some(TfmChar { width: 0.2906360626220703, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x7D
        None, // 0x7E
        Some(TfmChar { width: 0.46735477447509766, height: 0.6919641494750977, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};

/// `eufm5.tfm`.
#[rustfmt::skip]
pub static EUFM5: TfmFont = TfmFont {
    name: "eufm5",
    space: 0.33333396911621094,
    skew_char: None,
    params: &[0.0, 0.33333396911621094, 0.0, 0.0, 0.47802734375, 1.0, 0.0, 0.3802490234375, 0.2716064453125, 0.29876708984375, 0.3802490234375, 0.1629638671875, 0.40740966796875, 0.3802490234375, 0.325927734375, 0.19012451171875, 0.24444580078125, 0.40740966796875, 0.02716064453125, 2.210873603820801, 1.00494384765625, 0.258026123046875],
    chars: &[
        Some(TfmChar { width: 0.7899532318115234, height: 0.62469482421875, depth: 0.0, italic: 0.0 }), // 0x00
        Some(TfmChar { width: 0.790924072265625, height: 0.62469482421875, depth: 0.0, italic: 0.0 }), // 0x01
        Some(TfmChar { width: 0.5932741165161133, height: 0.6953125, depth: 0.19012451171875, italic: 0.0 }), // 0x02
        Some(TfmChar { width: 0.5884218215942383, height: 0.6953125, depth: 0.19012451171875, italic: 0.0 }), // 0x03
        Some(TfmChar { width: 0.7970705032348633, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x04
        Some(TfmChar { width: 0.5929508209228516, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x05
        Some(TfmChar { width: 0.5942440032958984, height: 0.62469482421875, depth: 0.0, italic: 0.0 }), // 0x06
        Some(TfmChar { width: 0.7938356399536133, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x07
        None, // 0x08
        None, // 0x09
        None, // 0x0A
        None, // 0x0B
        None, // 0x0C
        None, // 0x0D
        None, // 0x0E
        None, // 0x0F
        None, // 0x10
        None, // 0x11
        Some(TfmChar { width: 0.451263427734375, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x12
        Some(TfmChar { width: 0.451263427734375, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x13
        None, // 0x14
        None, // 0x15
        None, // 0x16
        None, // 0x17
        None, // 0x18
        None, // 0x19
        None, // 0x1A
        None, // 0x1B
        None, // 0x1C
        None, // 0x1D
        None, // 0x1E
        None, // 0x1F
        None, // 0x20
        Some(TfmChar { width: 0.548309326171875, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x21
        None, // 0x22
        None, // 0x23
        None, // 0x24
        None, // 0x25
        Some(TfmChar { width: 1.0778560638427734, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x26
        Some(TfmChar { width: 0.448028564453125, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x27
        Some(TfmChar { width: 0.6595888137817383, height: 0.7537078857421875, depth: 0.2512359619140625, italic: 0.0 }), // 0x28
        Some(TfmChar { width: 0.6595888137817383, height: 0.7537078857421875, depth: 0.2512359619140625, italic: 0.0 }), // 0x29
        Some(TfmChar { width: 0.5266361236572266, height: 0.62469482421875, depth: 0.0, italic: 0.0 }), // 0x2A
        Some(TfmChar { width: 1.099853515625, height: 0.5861263275146484, depth: 0.08365440368652344, italic: 0.0 }), // 0x2B
        Some(TfmChar { width: 0.5266361236572266, height: 0.108642578125, depth: 0.0, italic: 0.0 }), // 0x2C
        Some(TfmChar { width: 1.099853515625, height: 0.5861263275146484, depth: 0.08365440368652344, italic: 0.0 }), // 0x2D
        Some(TfmChar { width: 0.5266361236572266, height: 0.108642578125, depth: 0.0, italic: 0.0 }), // 0x2E
        Some(TfmChar { width: 0.7951297760009766, height: 0.7537078857421875, depth: 0.2512359619140625, italic: 0.0 }), // 0x2F
        Some(TfmChar { width: 0.7951297760009766, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x30
        Some(TfmChar { width: 0.7951297760009766, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x31
        Some(TfmChar { width: 0.7951297760009766, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x32
        Some(TfmChar { width: 0.7951297760009766, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x33
        Some(TfmChar { width: 0.7951297760009766, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x34
        Some(TfmChar { width: 0.7951297760009766, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x35
        Some(TfmChar { width: 0.7951297760009766, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x36
        Some(TfmChar { width: 0.7951297760009766, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x37
        Some(TfmChar { width: 0.7951297760009766, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x38
        Some(TfmChar { width: 0.7951297760009766, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x39
        Some(TfmChar { width: 0.452880859375, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x3A
        Some(TfmChar { width: 0.452880859375, height: 0.47802734375, depth: 0.12674903869628906, italic: 0.0 }), // 0x3B
        None, // 0x3C
        Some(TfmChar { width: 1.099853515625, height: 0.3707427978515625, depth: -0.1317291259765625, italic: 0.0 }), // 0x3D
        None, // 0x3E
        Some(TfmChar { width: 0.6282100677490234, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x3F
        None, // 0x40
        Some(TfmChar { width: 1.0535945892333984, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x41
        Some(TfmChar { width: 1.2528629302978516, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x42
        Some(TfmChar { width: 0.9277591705322266, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x43
        Some(TfmChar { width: 1.1901063919067383, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x44
        Some(TfmChar { width: 0.9879274368286133, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x45
        Some(TfmChar { width: 0.9261417388916016, height: 0.6953125, depth: 0.12674903869628906, italic: 0.0 }), // 0x46
        Some(TfmChar { width: 1.1347904205322266, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x47
        Some(TfmChar { width: 1.0568294525146484, height: 0.6953125, depth: 0.06337547302246094, italic: 0.0 }), // 0x48
        Some(TfmChar { width: 0.8582096099853516, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x49
        Some(TfmChar { width: 0.855621337890625, height: 0.6953125, depth: 0.12674903869628906, italic: 0.0 }), // 0x4A
        Some(TfmChar { width: 0.994720458984375, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x4B
        Some(TfmChar { width: 0.9918088912963867, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x4C
        Some(TfmChar { width: 1.4511594772338867, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x4D
        Some(TfmChar { width: 1.1907529830932617, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x4E
        Some(TfmChar { width: 1.1846065521240234, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x4F
        Some(TfmChar { width: 1.1852540969848633, height: 0.6953125, depth: 0.19012451171875, italic: 0.0 }), // 0x50
        Some(TfmChar { width: 1.1846065521240234, height: 0.6953125, depth: 0.03802490234375, italic: 0.0 }), // 0x51
        Some(TfmChar { width: 1.1859006881713867, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x52
        Some(TfmChar { width: 1.1865482330322266, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x53
        Some(TfmChar { width: 0.9953670501708984, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x54
        Some(TfmChar { width: 0.9675474166870117, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x55
        Some(TfmChar { width: 1.1897830963134766, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x56
        Some(TfmChar { width: 1.4469547271728516, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x57
        Some(TfmChar { width: 1.0555362701416016, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x58
        Some(TfmChar { width: 1.1917238235473633, height: 0.6953125, depth: 0.19012451171875, italic: 0.0 }), // 0x59
        Some(TfmChar { width: 0.9151430130004883, height: 0.6953125, depth: 0.12674903869628906, italic: 0.0 }), // 0x5A
        Some(TfmChar { width: 0.5266361236572266, height: 0.7537078857421875, depth: 0.2512359619140625, italic: 0.0 }), // 0x5B
        None, // 0x5C
        Some(TfmChar { width: 0.5266361236572266, height: 0.7537078857421875, depth: 0.2512359619140625, italic: 0.0 }), // 0x5D
        Some(TfmChar { width: 0.79254150390625, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x5E
        None, // 0x5F
        None, // 0x60
        Some(TfmChar { width: 0.7935123443603516, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x61
        Some(TfmChar { width: 0.8087158203125, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x62
        Some(TfmChar { width: 0.6605587005615234, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x63
        Some(TfmChar { width: 0.7912473678588867, height: 0.62469482421875, depth: 0.0, italic: 0.0 }), // 0x64
        Some(TfmChar { width: 0.6738224029541016, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x65
        Some(TfmChar { width: 0.5848636627197266, height: 0.6953125, depth: 0.19012451171875, italic: 0.0 }), // 0x66
        Some(TfmChar { width: 0.797393798828125, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x67
        Some(TfmChar { width: 0.81842041015625, height: 0.6953125, depth: 0.19012451171875, italic: 0.0 }), // 0x68
        Some(TfmChar { width: 0.5282535552978516, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x69
        Some(TfmChar { width: 0.530517578125, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x6A
        Some(TfmChar { width: 0.6605587005615234, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x6B
        Some(TfmChar { width: 0.528900146484375, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x6C
        Some(TfmChar { width: 1.1124696731567383, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x6D
        Some(TfmChar { width: 0.82489013671875, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x6E
        Some(TfmChar { width: 0.77960205078125, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x6F
        Some(TfmChar { width: 0.7935123443603516, height: 0.52691650390625, depth: 0.19012451171875, italic: 0.0 }), // 0x70
        Some(TfmChar { width: 0.7799253463745117, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x71
        Some(TfmChar { width: 0.6602354049682617, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x72
        Some(TfmChar { width: 0.7242860794067383, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x73
        Some(TfmChar { width: 0.5929508209228516, height: 0.62469482421875, depth: 0.0, italic: 0.0 }), // 0x74
        Some(TfmChar { width: 0.813568115234375, height: 0.47802734375, depth: 0.0, italic: 0.0 }), // 0x75
        Some(TfmChar { width: 0.807098388671875, height: 0.52691650390625, depth: 0.0, italic: 0.0 }), // 0x76
        Some(TfmChar { width: 1.1205568313598633, height: 0.52691650390625, depth: 0.0, italic: 0.0 }), // 0x77
        Some(TfmChar { width: 0.6595888137817383, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x78
        Some(TfmChar { width: 0.7915706634521484, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x79
        Some(TfmChar { width: 0.6618528366088867, height: 0.47802734375, depth: 0.19012451171875, italic: 0.0 }), // 0x7A
        None, // 0x7B
        None, // 0x7C
        Some(TfmChar { width: 0.451263427734375, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x7D
        None, // 0x7E
        Some(TfmChar { width: 0.64697265625, height: 0.6953125, depth: 0.0, italic: 0.0 }), // 0x7F
    ],
    kerns: &[
    ],
};
