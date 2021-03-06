use anyhow::*;
use bvh_anim::*;
use diva_db::bone::*;
use diva_db::mot::*;
use mot::*;
use structopt::StructOpt;

use std::path::PathBuf;

#[derive(Debug, StructOpt)]
#[structopt(name = "example", about = "An example of StructOpt usage.")]
struct Opt {
    #[structopt(parse(from_os_str))]
    mot_db: PathBuf,

    #[structopt(parse(from_os_str))]
    bone_db: PathBuf,

    /// Input file
    #[structopt(parse(from_os_str))]
    input: PathBuf,

    #[structopt(parse(from_os_str))]
    bvh: PathBuf,

    #[structopt(parse(from_os_str))]
    output: PathBuf,

    #[structopt(default_value = "+x+z+y")]
    orientation: String,

    offset: Option<usize>,
}

use std::fs::File;
use std::io::{self, Read};

use mot::read::DeserializeEndian;
use nom::number::Endianness;

use cookie_factory::*;

use env_logger::*;

fn main() -> Result<()> {
    let env = Env::default()
        .filter_or("MY_LOG_LEVEL", "trace")
        .write_style_or("MY_LOG_STYLE", "always");

    env_logger::init_from_env(env);

    info!("starting up");

    let opt = Opt::from_args();
    let mut file = File::open(&opt.input).context("failed to open mot file")?;
    let mut data = vec![];
    file.read_to_end(&mut data).context("failed to read mot file")?;

    let (_, mut mot) = Motion::parse(&data, Endianness::Little).unwrap();

    let mut file = File::open(opt.bvh).context("failed to open bvh")?;
    let mut data = vec![];
    file.read_to_end(&mut data).context("failed to read bvh")?;
    let bvh = from_bytes(&data[..])?;

    let mut file = File::open(opt.mot_db).context("failed to open mot_db")?;
    let mut data = vec![];
    file.read_to_end(&mut data)
        .context("failed to read mot_db")?;
    let (_, motset_db) = MotionSetDatabase::read(Endianness::Little)(&data[..]).unwrap();

    let mut file = File::open(opt.bone_db).context("failed to open bone_db")?;
    let mut data = vec![];
    file.read_to_end(&mut data)
        .context("failed to read bone_db")?;
    let (_, bone_db) = BoneDatabase::read(&data[..]).unwrap();

    for mut set in mot.sets.iter_mut() {
        *set = match set {
            FrameData::None | FrameData::Pose(_) => continue,
            FrameData::Linear(l) => FrameData::Pose(l[0].value),
            FrameData::Smooth(l) => FrameData::Pose(l[0].keyframe.value),
        }
    }

    // let mut sets = vec![FrameData::None; BONE_IDX.len() * 3];
    // let mut bones = vec![];
    for joint in bvh.joints() {
        let name = joint.data().name();
        let bone_id = motset_db.bones.iter().position(|x| &x[..] == &name[..]);
        let mut bone_id = match bone_id {
            Some(n) => n,
            None => {
                error!("couldn't find bone `{}` in motset_db", name);
                continue;
            }
        };
        debug!("{}: {}", bone_id, name);
        if name.contains("e_") {
            bone_id += 1;
        }
        // bone_id = if bone_id < 5 { bone_id } else { bone_id +1 };
        // if bone_id == 13 || name == "e_kao_cp" {
        //     error!("-----------------HEAD TIME----------------");
        //     bone_id = 12;
        // }
        let bone_id = mot.bones.iter().position(|x| *x == bone_id);
        let mut bone_id = match bone_id {
            Some(n) => n,
            None => {
                error!("couldn't find bone `{}` in const table", name);
                continue;
            }
        };

        // match bone_id {
        //     0 | 1 => (),
        //     _ => bone_id += opt.offset.unwrap_or(0),
        // };
        let rot = bone_db.skeletons[0]
            .bones
            .iter()
            .find(|x| &x.name[..] == &name[..])
            .map(|x| x.mode)
            .unwrap_or(BoneType::Position)
            == BoneType::Rotation;
        // if !(name == "n_hara_cp" || name == "kg_hara_y") {
        //     continue;
        // }
        let skip = [8];
        if skip.iter().find(|x| **x == bone_id).is_some() {
            continue
        }
        warn!("adding {} at {}", name, bone_id);
        let [x, y, z] = convert_joint_default(&bvh, &joint, rot, &opt.orientation);
        // sets.push((bone_id, frame));
        mot.sets[3*bone_id+0] = x;
        mot.sets[3*bone_id+1] = y;
        mot.sets[3*bone_id+2] = z;
        // bones.push(id);
    }

    // sets.insert(2, FrameData::None);
    // sets.insert(2, FrameData::None);
    // sets.insert(2, FrameData::None);

    // bones[0] = 1;
    // bones[1] = 0;

    // let mut bones_idx: Vec<(usize, &str)> = motset_db.bones.iter().map(|x| &x[..]).enumerate().collect();
    // bones_idx.sort_by(|(_, s1), (_, s2)| s1.cmp(s2));
    // let bones = bones_idx.into_iter().map(|(i, _)| i).collect();

    // warn!("bone ids: {:?}", &BONE_IDX[..]);

    // let mot = Motion { sets, bones: BONE_IDX.to_vec() };

    let mut file = File::create(opt.output)?;
    // let mut save = vec![];

    // let out = gen(mot.pub_write(), save)?;
    // let save = out.0;

    // io::copy(&mut (&save[..]), &mut file)?;
    mot.write()(&mut file)?;

    Ok(())
}

fn convert(bvh: &Bvh, chan: &Channel, conv: f32, off: f32) -> Vec<Keyframe> {
    bvh.frames()
        .map(|i| i[chan])
        .map(|f| f * conv)
        .map(|f| f + off)
        .enumerate()
        .map(|(i, value)| Keyframe {
            frame: i as u16,
            value,
        })
        .collect()
}

fn convert33(
    bvh: &Bvh,
    chan: [&Channel; 3],
    (xcon, ycon, zcon): (f32, f32, f32),
    (xoff, yoff, zoff): (f32, f32, f32),
) -> [Vec<Keyframe>; 3] {
    let x = convert(bvh, chan[0], xcon, xoff);
    let y = convert(bvh, chan[1], ycon, yoff);
    let z = convert(bvh, chan[2], zcon, zoff);
    [x, y, z]
}

fn convert_joint(
    bvh: &Bvh,
    joint: &Joint,
    coor: &str,
    mut conv: (f32, f32, f32),
    off: (f32, f32, f32),
    rot: bool,
) -> [Vec<Keyframe>; 3] {
    let channels = joint.data().channels();
    let rot_chan = if rot && channels.len() > 3 { 3 } else { 0 };
    let x = channels[0 + rot_chan];
    let y = channels[1 + rot_chan];
    let z = channels[2 + rot_chan];
    if rot {
        let pi = std::f32::consts::PI;
        let deg = pi / 180.;
        conv = (deg * conv.0, deg * conv.1, deg * conv.2);
        debug!("bone is rot");
    }
    match &coor[0..1] {
        "0" => conv.0 = 0.,
        "-" => conv.0 = conv.0 * -1.,
        "+" => (),
        e => panic!("encountered unexpected sign `{}`", e)
    }
    match &coor[2..3] {
        "0" => conv.1 = 0.,
        "-" => conv.1 = conv.1 * -1.,
        "+" => (),
        e => panic!("encountered unexpected sign `{}`", e)
    }
    match &coor[4..5] {
        "0" => conv.2 = 0.,
        "-" => conv.2 = conv.2 * -1.,
        "+" => (),
        e => panic!("encountered unexpected sign `{}`", e)
    }
    let x = match &coor[1..2] {
        "x" => x,
        "y" => y,
        "z" => z,
        e => panic!("encountered unexpected axis `{}`", e)
    };
    let y = match &coor[3..4] {
        "x" => x,
        "y" => y,
        "z" => z,
        e => panic!("encountered unexpected axis `{}`", e)
    };
    let z = match &coor[5..6] {
        "x" => x,
        "y" => y,
        "z" => z,
        e => panic!("encountered unexpected axis `{}`", e)
    };
    convert33(&bvh, [&x, &z, &y], conv, off)
}

fn convert_joint_default(bvh: &Bvh, joint: &Joint, rot: bool, coor: &str) -> [FrameData; 3] {
    let scale = 1.0;
    let conv = (scale * 1., scale * 1., scale * -1.);
    let off = (0., 0., 0.);
    let [x, y, z] = convert_joint(bvh, joint, coor, conv, off, rot);
    [
        FrameData::Linear(x),
        FrameData::Linear(y),
        FrameData::Linear(z),
    ]
}

use log::*;

fn set_joint(sets: &mut Vec<FrameData>, bvh: &Bvh, joint_name: &str, coor: &str, id: usize, rot: bool) {
    let joint = bvh.joints().find_by_name(joint_name);
    let joint = match joint {
        Some(j) => j,
        None => {
            warn!("Could not find `{}` in the bvh file, ignoring", joint_name);
            return;
        }
    };
    let pi = std::f32::consts::PI;
    let deg = pi / 180.;
    let conv = if rot { (deg, deg, -deg) } else { (1., 1., 1.) };
    let off = (0., 0., 0.);
    let [x, y, z] = convert_joint(bvh, &joint, coor, conv, off, rot);
    sets[3 * id + 0] = FrameData::Linear(x);
    sets[3 * id + 1] = FrameData::Linear(y);
    sets[3 * id + 2] = FrameData::Linear(z);
}
