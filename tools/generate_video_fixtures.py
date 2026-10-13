"""Original synthetic H.264/SDR fixtures. Requires host ffmpeg and ffprobe."""
from pathlib import Path
import json, math, struct, subprocess, tempfile, wave, zlib

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'crates/motion-media/tests/fixtures/video'
OUT.mkdir(parents=True, exist_ok=True)
WIDTH, HEIGHT = 256, 144
COLORS = [(230, 30, 30), (30, 220, 30), (30, 30, 230), (220, 220, 30)]

def pixels(index):
    data = bytearray()
    for y in range(HEIGHT):
        for x in range(WIDTH):
            color = COLORS[(index // 12) % 4]
            if x < 32: color = (240, 240, 240) if y < HEIGHT // 2 else (20, 20, 20)
            if x >= WIDTH - 32: color = (200, 20, 180) if y < HEIGHT // 2 else (20, 180, 200)
            if y < 8 and 40 <= x < 136: color = (240, 240, 240) if (index >> ((x-40)//16)) & 1 else (20, 20, 20)
            data.extend(color)
    return data

def run(*args):
    subprocess.run(['ffmpeg', '-hide_banner', '-loglevel', 'error', '-y', *map(str,args)], check=True)

def compressed_movie(source, target):
    # Keep mdat/chunk positions intact by replacing the final moov in place.
    data=source.read_bytes(); offset=0
    while offset<len(data):
        size,kind=struct.unpack_from('>I4s',data,offset)
        if kind==b'moov':
            if offset+size!=len(data): raise RuntimeError('Compressed fixture requires final moov')
            body=data[offset+8:offset+size]
            def box(kind,body): return struct.pack('>I4s',len(body)+8,kind)+body
            cmov=box(b'cmov',box(b'dcom',b'zlib')+box(b'cmvd',struct.pack('>I',len(body))+zlib.compress(body)))
            target.write_bytes(data[:offset]+box(b'moov',cmov));return
        if size<8: raise RuntimeError('Invalid fixture box')
        offset+=size
    raise RuntimeError('Fixture movie metadata missing')

with tempfile.TemporaryDirectory(prefix='motion-video-fixtures-') as directory:
    temp = Path(directory)
    raw = temp / 'frames.rgb'
    with raw.open('wb') as f:
        for n in range(72): f.write(pixels(n))
    audio = temp / 'pulse.wav'
    with wave.open(str(audio), 'wb') as f:
        f.setnchannels(2); f.setsampwidth(2); f.setframerate(48000)
        for n in range(144000):
            t=n/48000
            active = 0.10 <= t < 0.30 or 1.0 <= t < 1.2 or 2.0 <= t < 2.2
            f.writeframesraw(struct.pack('<hh', int(10000*math.sin(2*math.pi*440*t)) if active else 0, int(8000*math.sin(2*math.pi*880*t)) if active else 0))
    options=['-c:v','libx264','-preset','fast','-pix_fmt','yuv420p','-g','24','-bf','2','-colorspace','bt709','-color_primaries','bt709','-color_trc','bt709','-color_range','tv','-video_track_timescale','1000000','-movflags','+faststart']
    video=OUT/'silent-24fps.mp4'
    run('-f','rawvideo','-pixel_format','rgb24','-video_size',f'{WIDTH}x{HEIGHT}','-framerate','24','-i',raw,*options,video)
    run('-i',video,'-i',audio,'-map','0:v:0','-map','1:a:0','-c:v','copy','-c:a','aac','-b:a','128k','-movflags','+faststart',OUT/'sound-24fps.mp4')
    run('-i',video,'-itsoffset','0.25','-i',audio,'-map','0:v:0','-map','1:a:0','-c:v','copy','-c:a','aac','-b:a','128k','-movflags','+faststart',OUT/'audio-delayed.mp4')
    # Nonzero video PTS with a leading empty edit, and B-frame ctts without edits.
    run('-itsoffset','0.25','-i',video,'-map','0:v:0','-c','copy','-video_track_timescale','1000000',OUT/'video-delayed.mp4')
    # A small ctts offset and shorter final sample expose a median-step tail
    # overrun; the muxer records the actual shortened sample duration in stts.
    run('-i',video,'-c','copy','-bsf:v','setts=pts=PTS-63333','-use_editlist','0','-video_track_timescale','1000000',OUT/'video-ctts.mp4')
    run('-i',OUT/'video-ctts.mp4','-map','0:v:0','-c','copy','-video_track_timescale','1000000','-movflags','+frag_keyframe+empty_moov+default_base_moof',OUT/'video-fragmented.mp4')
    no_b=temp/'no-b.mp4'
    run('-i',video,'-an','-c:v','libx264','-preset','fast','-bf','0','-pix_fmt','yuv420p','-vf','setpts=PTS+0.25/TB','-fps_mode','passthrough','-video_track_timescale','90000',no_b)
    # Introduce VFR and a sub-frame tail. Max PTS is the final decode sample;
    # rounding PTS and duration independently loses one microsecond here.
    expr=r'setts=pts=PTS+if(gte(N\,48)\,2250\,0):dts=DTS+if(gte(N\,48)\,2250\,0):duration=if(eq(N\,71)\,902\,DURATION)'
    run('-copyts','-i',no_b,'-c','copy','-bsf:v',expr,'-video_track_timescale','90000',OUT/'video-tail-90k.mp4')
    compressed_movie(OUT/'video-tail-90k.mp4',OUT/'video-tail-cmov.mov')
    run('-display_rotation','-90','-i',video,'-c','copy',OUT/'rotated-90.mp4')
    durations=[.04,.08,.12,.20,.04,.12,.08,.16,.04,.20,.08,.12]
    lines=[]
    for n,duration in enumerate(durations):
        ppm=temp/f'{n}.ppm';ppm.write_bytes(f'P6\n{WIDTH} {HEIGHT}\n255\n'.encode()+pixels(n*6))
        lines.extend([f"file '{ppm.as_posix()}'",f'duration {duration}'])
    lines.append(f"file '{(temp/'11.ppm').as_posix()}'")
    concat=temp/'vfr.txt';concat.write_text('\n'.join(lines)+'\n',encoding='utf-8')
    run('-f','concat','-safe','0','-i',concat,'-fps_mode','vfr',*options,OUT/'variable.mp4')

manifest={}
for path in sorted(list(OUT.glob('*.mp4'))+list(OUT.glob('*.mov'))):
    result=subprocess.check_output(['ffprobe','-v','error','-select_streams','v:0','-show_frames','-show_streams','-show_entries','frame=best_effort_timestamp_time,pkt_duration_time:stream=width,height,avg_frame_rate,r_frame_rate,duration:stream_side_data=rotation','-of','json',str(path)])
    manifest[path.name]=json.loads(result)
    if path.name.startswith('video-'):
        result=json.loads(subprocess.check_output(['ffprobe','-v','error','-select_streams','v:0','-show_packets','-show_streams','-show_entries','packet=pts,dts,duration,pos,size:stream=time_base','-of','json',str(path)]))
        from fractions import Fraction
        tail=max(result['packets'],key=lambda p:p['pts'])
        base=result['streams'][0]['time_base'];end=(tail['pts']+tail['duration'])*Fraction(base)*1_000_000
        manifest[path.name]['video_end_us']=(end.numerator*2+end.denominator)//(2*end.denominator)
        manifest[path.name]['tail_sample']={**tail,'time_base':base}
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(json.dumps({p.name:p.stat().st_size for p in sorted(OUT.glob('*.mp4'))},indent=2))
