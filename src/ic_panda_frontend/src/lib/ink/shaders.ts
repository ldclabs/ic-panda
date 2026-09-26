/*
  GLSL for the ink painting behind the landing page. The ink engine (splat →
  bleed → paper → display) is adapted from 墨韵 Moyun, MIT License,
  Copyright (c) 2026 Axton Liu. The fluid solver is left out on purpose:
  nothing here moves the ink, so the look comes entirely from the bleed pass.
*/

export const VS = `#version 300 es
precision highp float;
layout(location=0) in vec2 aPos;
uniform vec2 texel;
out vec2 vUv; out vec2 vL; out vec2 vR; out vec2 vT; out vec2 vB;
void main(){
  vUv=aPos*.5+.5;
  vL=vUv-vec2(texel.x,0.); vR=vUv+vec2(texel.x,0.);
  vT=vUv+vec2(0.,texel.y); vB=vUv-vec2(0.,texel.y);
  gl_Position=vec4(aPos,0.,1.);
}`
export const HEAD = `#version 300 es
precision highp float; precision highp sampler2D; precision highp int;
in vec2 vUv; in vec2 vL; in vec2 vR; in vec2 vT; in vec2 vB;
out vec4 o;
uint pcg(uint v){uint s=v*747796405u+2891336453u;uint w=((s>>((s>>28u)+4u))^s)*277803737u;return (w>>22u)^w;}
float hash2(vec2 p){ivec2 i=ivec2(floor(p));return float(pcg(uint(i.x)+pcg(uint(i.y)+7919u)))*(1./4294967295.);}
float h1(float x){return float(pcg(uint(int(floor(x))+65536)))*(1./4294967295.);}
float vnoise(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);
  return mix(mix(hash2(i),hash2(i+vec2(1,0)),f.x),mix(hash2(i+vec2(0,1)),hash2(i+vec2(1,1)),f.x),f.y);}
float vn1(float x){float i=floor(x),f=fract(x);f=f*f*(3.-2.*f);return mix(h1(i),h1(i+1.),f);}
float fbm(vec2 p){float s=0.,a=.5;for(int k=0;k<5;k++){s+=a*vnoise(p);p=p*2.02+vec2(17.3,9.1);a*=.5;}return s;}
`
export const FS = {
  /* Dye channels: x ink, y cinnabar, z water, w mineral green (石绿). */
  splat: `
uniform sampler2D uTarget; uniform float aspect; uniform int count; uniform vec2 res; uniform float lim;
uniform vec4 uPos[32]; uniform vec4 uVal[32]; uniform vec4 uDir[32];
void main(){
  vec4 base=texture(uTarget,vUv), add=vec4(0.);
  for(int i=0;i<32;i++){
    if(i>=count) break;
    vec2 p=vUv-uPos[i].xy; p.x*=aspect;
    float q=dot(p,p)/uPos[i].z;
    if(q>40.) continue;
    float g=exp(-pow(q,1.+uPos[i].w*3.));
    float dry=uVal[i].w;
    if(dry>.001){
      vec2 n=vec2(-uDir[i].y,uDir[i].x);
      float s=dot(p,n)*res.y*.5+uDir[i].z;
      float br=vn1(s)*.65+vn1(s*2.3+11.)*.35;
      float cut=dry*.8;
      g*=mix(1.,smoothstep(cut,cut+.2,br),min(1.,dry*1.4));
    }
    add.xy+=g*uVal[i].xy;
    add.w+=g*uDir[i].w;
    add.z+=exp(-q*.35)*uVal[i].z;
  }
  o=clamp(base+add,-lim,lim);
}`,
  bleed: `
uniform sampler2D uDye; uniform float rate; uniform float evap; uniform vec2 res;
void main(){
  vec4 c=texture(uDye,vUv), l=texture(uDye,vL), r=texture(uDye,vR), t=texture(uDye,vT), b=texture(uDye,vB);
  vec4 avg=(l+r+t+b)*.25;
  float wet=clamp(max(c.z,avg.z),0.,1.);
  vec2 px=vUv*res;
  float fib=vnoise(px*.45)*.55+vnoise(px*vec2(.05,.3))*.45;
  float k=clamp(rate*wet*mix(.1,1.,fib),0.,1.);
  vec4 n=c;
  n.xy+=(avg.xy-c.xy)*k*.45;
  n.w+=(avg.w-c.w)*k*.3;
  n.z+=(avg.z-c.z)*clamp(k*1.3,0.,1.);
  n.z*=evap;
  o=max(n,vec4(0.));
}`,
  clear: `
uniform sampler2D uTexture; uniform float value;
void main(){ o=value*texture(uTexture,vUv); }`,
  copy: `
uniform sampler2D uTexture;
void main(){ o=texture(uTexture,vUv); }`,
  paper: `
uniform vec2 res; uniform vec3 paper; uniform vec3 paper2; uniform float seed;
float fibers(vec2 px){
  mat2 r1=mat2(.8,.6,-.6,.8), r2=mat2(.28,-.96,.96,.28), r3=mat2(-.5,.87,-.87,-.5);
  float f=smoothstep(.66,.95,vnoise(r1*px*vec2(.018,.42)+seed));
  f+=.8*smoothstep(.68,.96,vnoise(r2*px*vec2(.014,.5)+seed*1.7));
  f+=.6*smoothstep(.7,.97,vnoise(r3*px*vec2(.025,.6)+seed*.3));
  return f;
}
void main(){
  vec2 px=vUv*res;
  float cloud=fbm(px*.0035+seed);
  float fib=fibers(px);
  float spec=hash2(px*.5+seed);
  vec3 c=mix(paper,paper2,clamp(cloud*.55-.08,0.,1.));
  c=mix(c,paper2,clamp(fib*.22,0.,1.));
  c=mix(c,paper2,step(.996,spec)*.6);
  float gran=vnoise(px*.55)*.6+vnoise(px*.13)*.4;
  o=vec4(c,gran);
}`,
  disp: `
uniform sampler2D uDye; uniform sampler2D uPaper; uniform vec2 dtex; uniform vec3 ink; uniform vec3 cin; uniform vec3 jade;
void main(){
  vec4 pp=texture(uPaper,vUv); vec4 d=texture(uDye,vUv);
  float l=texture(uDye,vUv-vec2(dtex.x,0.)).x, r=texture(uDye,vUv+vec2(dtex.x,0.)).x;
  float t=texture(uDye,vUv+vec2(0.,dtex.y)).x, b=texture(uDye,vUv-vec2(0.,dtex.y)).x;
  float grad=length(vec2(r-l,t-b));
  float gran=.8+.4*pp.a;
  float k=max(d.x,0.)*gran;
  float dens=1.-exp(-k*2.3);
  dens=clamp(dens+min(grad*.8,.25)*(1.-dens)*smoothstep(.02,.2,k),0.,1.);
  vec3 col=mix(pp.rgb,ink,dens);
  float g=1.-exp(-max(d.w,0.)*2.2*gran);
  col=mix(col,mix(jade,ink,dens*.5),g*.7);
  float v=1.-exp(-max(d.y,0.)*2.6*gran);
  col=mix(col,mix(cin,ink,dens*.35),v*.93);
  col*=1.-.045*clamp(d.z*.6,0.,1.);
  vec2 q=vUv-.5; col*=1.-.09*dot(q,q);
  o=vec4(col,1.);
}`,
  /*
  The panda: 写意 style. The white body is paper (留白) under a light wash of
  shell white (蛤粉); only the ears, eye patches, legs and the black coat
  carry ink. Parts are signed distance shapes in panda space (origin on the
  ground under her seat, height ≈ 1), layered like a painter would: white
  fills erase what is under them, ink accumulates. The result goes through
  the same density curve as the fluid ink so the two layers read as one
  sheet of paper.

  Proportions follow photos of a sitting giant panda eating: a big round head
  with wide cheeks and small ears on its corners, small drooping eye patches
  with round bright eyes, no shoulders, black flanks with only the belly
  white between them, one paw gripping the stalk low and the other forearm
  bringing its end into her mouth. Head turns move the face features across
  a nearly still outline (parallax), which reads as a turn instead of a spin.
*/
  panda: `
uniform sampler2D uPaper; uniform vec2 res;
uniform vec4 uP;
uniform vec3 ink; uniform vec3 jade;
uniform vec4 uBody;   // breath
uniform vec4 uHead;   // turn, nod, tilt, jaw
uniform vec4 uHeadO;  // head offset x, y
uniform vec4 uEar;    // left, right ear angle
uniform vec4 uEye;    // gaze x, gaze y, open, wide
uniform vec4 uArm;    // eating paw offset x, y, stalk angle, leaf rustle
uniform float uRev[12];
float K, G, Wm, AA, N1, N2, N3, N4, FIB;
mat2 rot(float a){float c=cos(a),s=sin(a);return mat2(c,s,-s,c);}
float sdE(vec2 p, vec2 r){return (length(p/r)-1.)*min(r.x,r.y);}
float smin(float a,float b,float k){float h=clamp(.5+.5*(b-a)/k,0.,1.);return mix(b,a,h)-k*h*(1.-h);}
float ez(float x){x=clamp(x,0.,1.);return x*x*(3.-2.*x);}
void fill(float d, float a){float m=smoothstep(AA,-AA,d)*a;K*=1.-m;G*=1.-m;Wm=max(Wm,m);}
/* Wet ink with a furred edge: darker rim where pigment piles up, fine hairs
   and a faint halo into the paper. */
float blob(float d, float rev, float grow, float tone, float rough){
  if(rev<=0.) return 0.;
  d+=(1.-ez(rev))*grow+rough*((N1-.5)*.01+(N2-.5)*.006+(N4-.5)*.005);
  float inside=smoothstep(AA,-AA,d);
  float rim=exp(-max(-d,0.)/(AA*4.));
  float halo=exp(-max(d,0.)/(AA*1.8))*(1.-inside)*FIB*(.08+.3*smoothstep(.3,.8,N3));
  return tone*(inside*(.8+.45*(N3-.5)+.55*rim)+halo);
}
/* 飞白: bristle streaks across a stroke; s runs -1..1 across its width. */
float bristle(float s, float dry, float seed){
  float b=vn1(s*6.5+seed)*.6+vn1(s*14.+seed*1.7)*.4;
  float cut=dry*.8;
  return mix(1.,smoothstep(cut,cut+.22,b),min(1.,dry*1.3));
}
/* One brush stroke from a to b, revealed from a to b as rev goes 0 → 1. */
float limb(vec2 p, vec2 a, vec2 b, float ra, float rb, float rev, float tone, float dry, float seed, float rough){
  if(rev<=0.) return 0.;
  vec2 ba=b-a; float L=length(ba); vec2 u=ba/L; vec2 pa=p-a;
  float s=dot(pa,u)/L, t=clamp(s,0.,1.);
  float r=mix(ra,rb,t);
  float d=length(pa-u*t*L)-r;
  if(d>.03) return 0.;
  float k=blob(d,1.,0.,tone,rough);
  k*=bristle(dot(pa,vec2(-u.y,u.x))/r,dry*smoothstep(.3,1.,t),seed);
  float e=rev*1.15;
  return k*(1.-smoothstep(e-.12,e,s));
}
/* Light dry outline along a contour; ang runs 0..1 around it, gap sets how
   often the brush lifts off and how dry it runs. */
float outline(float d, float ang, float w, float rev, float seed, float tone, float gap){
  if(rev<=0.) return 0.;
  w=max(w,AA*1.4);
  float a=abs(d+w*.4+(N2-.5)*.004)/w;
  if(a>2.) return 0.;
  float core=smoothstep(1.,.3,a);
  core*=bristle(d/w,(.35+.45*vn1(ang*9.+seed))*(gap+.15)*2.,seed);
  float gaps=smoothstep(gap,gap+.2,vn1(ang*6.+seed*2.3));
  float e=rev*1.06;
  return tone*core*gaps*(1.-smoothstep(e-.05,e,ang))*(.7+.5*vn1(ang*23.+seed));
}
/* Wet side-brush ink (侧锋) for the limbs: soft bleeding edges instead of a
   torn one, a heavy edge where the brush tip ran and a lighter one opposite,
   pooling, and bristle streaks along the stroke. */
float wet(float d, float x, float side, float dry, float seed){
  d+=(N1-.5)*.012+(N2-.5)*.003;
  float inside=smoothstep(AA*1.3,-AA*1.3,d);
  float rim=exp(-max(-d,0.)/(AA*5.));
  float halo=exp(-max(d,0.)/(AA*2.8))*(1.-inside)*FIB*(.12+.28*smoothstep(.3,.8,N3));
  float tone=(.78+.4*(N3-.5)+.45*rim+.25*smoothstep(.62,.9,N1))*mix(.74,1.1,smoothstep(-1.,1.,x*side));
  return 1.05*(inside*tone*bristle(x,dry,seed)+halo);
}
/* Bamboo leaf: belly near the base, long taper to a dry tip. */
float leaf(vec2 p, vec2 base, float ang, float len, float wid, float droop, float rev){
  if(rev<=0.) return 0.;
  vec2 u=vec2(cos(ang),sin(ang)), v=vec2(-u.y,u.x);
  vec2 q=p-base;
  float s=dot(q,u)/len;
  if(s<-.05||s>1.05) return 0.;
  q.y+=droop*len*s*s;
  float y=dot(q,v);
  float w=wid*sin(3.14159*sqrt(clamp(s,0.,1.)));
  float d=abs(y)-w;
  if(d>.02) return 0.;
  float inside=smoothstep(AA,-AA,d+(N2-.5)*.004);
  float k=inside*(.85+.4*(N3-.5)+.35*exp(-max(-d,0.)/(AA*3.)));
  k*=bristle(y/max(w,1e-3),.45*smoothstep(.4,1.,s),ang*13.);
  float e=rev*1.12;
  return k*(1.-smoothstep(e-.12,e,s));
}
void main(){
  vec2 frag=gl_FragCoord.xy;
  vec2 b=(frag-uP.xy)/uP.z;
  AA=1.3/uP.z;
  N1=vnoise(b*24.+3.1); N2=vnoise(b*70.+11.7); N3=vnoise(b*7.+5.3); N4=vnoise(b*160.+2.9);
  FIB=vnoise(b*170.)*.55+vnoise(b*vec2(25.,140.))*.45;
  K=0.; G=0.; Wm=0.;
  float fillA=ez(max(uRev[0],uRev[4])*4.);

  /* ground shadow */
  { float d=sdE(b-vec2(-.06,0.),vec2(.58,.045))+(N1-.5)*.03;
    K+=smoothstep(.01,-.04,d)*.2*ez(uRev[11])*(.6+.6*N3); }

  /* body: a wide seat and a bell-shaped upper body. A panda has no
     shoulders: from beside the head the outline runs straight down and out
     to her widest point, and the white back rises behind the head. */
  vec2 u=b-vec2(0.,.45);
  float rx=(.46-.35*max(u.y,0.))*(1.+uBody.x);
  float dB=smin(sdE(b-vec2(0.,.22),vec2(.42,.23)),sdE(vec2(u.x*.46/rx,u.y),vec2(.46,.35)),.06);
  fill(dB,fillA);
  { vec2 q=b-vec2(0.,.3);
    K+=outline(dB,fract((atan(q.y,q.x)-2.8)/6.2832),.006,uRev[0],1.3,.5,.1); }

  /* head: a big round face with wide cheeks; it tilts about the neck, the
     outline barely moves on a turn, the ears move against it and the
     features slide across the face */
  float turn=uHead.x, nod=uHead.y, jaw=uHead.w;
  vec2 Nk=vec2(0.,.6);
  vec2 hb=rot(-uHead.z)*(b-Nk)+Nk-uHeadO.xy;
  vec2 hO=hb-vec2(turn*.012,nod*.008);
  vec2 hE=hb+vec2(turn*.014,nod*.01);
  vec2 hF=hb-vec2(turn*.05,nod*.03);
  /* small round ears set on the top corners */
  { vec2 bL=vec2(-.17,.9), bR=vec2(.17,.9);
    vec2 qL=rot(-uEar.x)*(hE-bL)+bL, qR=rot(-uEar.y)*(hE-bR)+bR;
    K+=blob(sdE(qL-vec2(-.212,.94),vec2(.064,.058)),uRev[3],.06,1.1,1.);
    K+=blob(sdE(qR-vec2(.212,.94),vec2(.064,.058)),uRev[3],.06,1.1,1.); }
  float dH=smin(sdE(hO-vec2(0.,.8),vec2(.25,.18)),sdE(hO-vec2(0.,.72),vec2(.285,.13)),.06);
  fill(dH,fillA);
  { vec2 q=hO-vec2(0.,.78);
    K+=outline(dH,fract((atan(q.y,q.x)-.3)/6.2832),.004,uRev[4],7.1,.45,.35); }

  /* eye patches: small drooping teardrops, narrowing on the side turned away */
  float fsL=1.-.3*max(-turn,0.), fsR=1.-.3*max(turn,0.);
  vec2 cL=vec2(-.083,.772), cR=vec2(.083,.772);
  { vec2 w=hF-cL; w.x/=fsL; vec2 q=rot(.75)*w;
    K+=blob(smin(sdE(q,vec2(.036,.05)),length(q-vec2(0.,-.026))-.035,.025),uRev[5],.04,.95,.6);
    w=hF-cR; w.x/=fsR; q=rot(-.75)*w;
    K+=blob(smin(sdE(q,vec2(.036,.05)),length(q-vec2(0.,-.026))-.035,.025),uRev[5],.04,.95,.6); }
  /* 点睛: round dark eyes with two catchlights, the last thing painted */
  if(uRev[10]>0.){
    float e=ez(uRev[10]), op=max(uEye.z,.06);
    float er=.0145*(1.+.15*uEye.w)*e;
    vec2 gz=uEye.xy*vec2(.008,.006);
    vec2 eL=cL+vec2(fsL,1.)*(rot(-.75)*vec2(0.,.014))+gz;
    vec2 eR=cR+vec2(fsR,1.)*(rot(.75)*vec2(0.,.014))+gz;
    K+=2.4*smoothstep(AA,-AA,length((hF-eL)/vec2(1.,op))-er);
    K+=2.4*smoothstep(AA,-AA,length((hF-eR)/vec2(1.,op))-er);
    float show=smoothstep(.45,.7,op), h1=.0052*(1.+.35*uEye.w)*e, h2=.0024*e;
    vec2 o1=vec2(.0045,.0055), o2=vec2(-.0045,-.0045);
    fill(length(hF-eL-o1)-h1,show); fill(length(hF-eR-o1)-h1,show);
    fill(length(hF-eL-o2)-h2,show*.9); fill(length(hF-eR-o2)-h2,show*.9);
  }
  /* The black coat, after photos of a panda eating. No shoulders: the black
     starts beside the head at cheek height and slopes straight out to her
     sides, inside her outline. Her flanks are broad black masses with only
     the belly white between them; the left one hangs down to a paw that
     grips the stalk low, the right one ends at the elbow, from which the
     forearm rises (painted later, in front of the stalk). */
  vec2 P=vec2(.09,.59)+uArm.xy, Lp=vec2(-.28,.25), E=vec2(.33,.33);
  { float y=b.y, r=ez(mix(uRev[9],uRev[2],smoothstep(.45,.6,y)));
    if(r>0.){
      float xl=y>.4?mix(-.215,-.17,smoothstep(.4,.62,y)):mix(-.2,-.215,smoothstep(.27,.4,y));
      float xr=mix(.24,.16,smoothstep(.3,.62,y));
      float armL=min(max(b.x-xl,.3-y),length(b-vec2(-.31,.31))-.1);
      float armR=min(max(xr-b.x,.3-y),length(b-E)-.1);
      float yTop=.73-.5*max(abs(b.x)-.26,0.);
      float coat=max(max(min(armL,armR),max(dB,(y-yTop)*.8)),-dH)+(1.-r)*.15;
      K+=wet(coat,clamp((abs(b.x)-.3)/.14,-1.,1.),1.,.06,11.);
    }
  }

  /* hind legs: splayed forward in front of her flanks, each an egg that
     swells into the foot, the sole a paler pad facing us */
  if(uRev[1]>0.){
    float r1=ez(uRev[1]*1.3), r2=ez(uRev[1]*1.6-.6);
    for(int i=0;i<2;i++){
      float sx=i==0?-1.:1.;
      vec2 q=vec2(b.x*sx,b.y);
      float d=smin(sdE(rot(.5)*(q-vec2(.27,.135)),vec2(.14,.1)),length(q-vec2(.355,.09))-.096,.05)+(1.-r1)*.12;
      fill(d,1.);
      float k=wet(d,(q.y-.13)/.1,1.,.06,3.+sx*2.);
      k*=1.-.42*r2*smoothstep(.012,-.018,sdE(rot(.4)*(q-vec2(.365,.085)),vec2(.05,.04)));
      K+=k;
    }
  }

  /* bamboo: its end in her mouth, held up to it by the right paw, gripped
     low by the left one, running across the belly and past her hip to a
     sprig of leaves */
  vec2 T=vec2(.02,.652)+uArm.xy*.4;
  vec2 dir=normalize(Lp-T), nrm=vec2(-dir.y,dir.x);
  vec2 Tt=T, Bt=T+dir*.82;
  if(uRev[7]>0.){
    float kb=0.;
    for(int i=0;i<4;i++){
      float t0=float(i)*.25+.01, t1=float(i+1)*.25-.01;
      kb+=limb(b,mix(Tt,Bt,t0),mix(Tt,Bt,t1),.018+.003*float(i)/3.,.019+.003*float(i)/3.,clamp(uRev[7]*4.-float(i),0.,1.),1.,.25,float(i)*3.1,.4);
    }
    K+=kb*.7; G+=kb*.55;
    for(int i=1;i<4;i++){
      vec2 c=mix(Tt,Bt,float(i)*.25);
      K+=limb(b,c-nrm*.026,c+nrm*.026,.005,.004,clamp(uRev[7]*4.-float(i)+.5,0.,1.),1.2,.3,float(i)*7.,.2);
    }
  }
  if(uRev[8]>0.){
    float r8=uRev[8]*1.6, rs=uArm.w, kl=0., th=atan(dir.y,dir.x);
    vec2 S=Bt-dir*.07;
    kl+=leaf(b,S,th+.55+rs,.19,.024,.3,clamp(r8,0.,1.));
    kl+=leaf(b,S,th-.25+rs*1.3,.17,.022,.25,clamp(r8-.2,0.,1.));
    kl+=leaf(b,S,th+1.2+rs*.8,.15,.021,.35,clamp(r8-.4,0.,1.));
    kl+=leaf(b,Bt,th+.2+rs*1.5,.12,.018,.2,clamp(r8-.6,0.,1.));
    K+=kl*1.1; G+=kl*.3;
  }

  /* the muzzle, which sticks out, so it slides most on a turn: its upper lip
     comes down over the end of the stalk so the stalk goes into her mouth,
     then the nose and a small smile */
  { vec2 q=hb-vec2(turn*.06,nod*.04);
    fill(sdE(q-vec2(0.,.668),vec2(.042,.02)),fillA);
    vec2 n=q-vec2(0.,.683);
    K+=blob(smin(sdE(n,vec2(.035,.02)),sdE(n+vec2(0.,.011),vec2(.018,.013)),.018),uRev[6],.03,1.25,.4);
    float r6=uRev[6], jo=jaw*.008;
    vec2 m1=vec2(0.,.648-jo), mL=vec2(-.015,.641-jo*.7), mR=vec2(.015,.641-jo*.7);
    K+=limb(q,vec2(0.,.664),m1,.0032,.0028,r6,.75,0.,3.,.2);
    K+=limb(q,m1,mL,.0028,.0026,r6,.7,.2,4.,.2);
    K+=limb(q,mL,vec2(-.03,.65),.0026,.0022,r6,.6,.3,5.,.2);
    K+=limb(q,m1,mR,.0028,.0026,r6,.7,.2,6.,.2);
    K+=limb(q,mR,vec2(.03,.65),.0026,.0022,r6,.6,.3,8.,.2);
    if(jaw>.05) K+=1.3*jaw*step(.5,r6)*smoothstep(AA,-AA,sdE(q-vec2(0.,.633-jo),vec2(.014,.003+.007*jaw))); }

  /* over the stalk: the right forearm rises from the elbow on her flank to a
     paw under the corner of her mouth, holding the stalk up to it; the left
     paw grips it low. Each is laid over what is under it like a fresh stroke. */
  if(uRev[9]>0.){
    float r=ez(uRev[9]*1.4-.4);
    vec2 ep=P-E, pe=b-E;
    float s=clamp(dot(pe,ep)/dot(ep,ep),0.,1.);
    float fore=smin(length(pe-ep*s)-mix(.1,.066,s),sdE(rot(-.5)*(b-P),vec2(.076,.068)),.02);
    float d=min(fore,sdE(rot(.35)*(b-Lp),vec2(.09,.074)))+(1.-r)*.09;
    fill(d+.008,1.);
    K+=1.08*wet(d,(b.y-.4)/.2,1.,.05,17.);
  }

  vec4 pp=texture(uPaper,frag/res);
  float gran=.8+.4*pp.a;
  float dens=1.-exp(-max(K,0.)*gran*2.3);
  float gv=(1.-exp(-max(G,0.)*2.2*gran))*.7;
  vec2 q=frag/res-.5; float vig=1.-.09*dot(q,q);
  /* 蛤粉: a wash of shell white over the paper of her coat, brightest on the
     face and belly where the light falls and fading back to paper at her
     outline, so the grain and the soft edges stay. */
  float light=clamp(max(1.-length((b-vec2(0.,.3))/vec2(.32,.26)),1.-length((hO-vec2(0.,.76))/vec2(.22,.16))),0.,1.);
  float lift=clamp((.5+.35*light)*(.85+.3*N3)*smoothstep(0.,.03,-min(dB,dH)),0.,1.);
  vec3 c=mix(pp.rgb*vig,vec3(.995,.992,.982),lift)*Wm; float a=Wm;
  c=c*(1.-dens)+ink*vig*dens; a=a*(1.-dens)+dens;
  c=c*(1.-gv)+mix(jade,ink,dens*.5)*vig*gv; a=a*(1.-gv)+gv;
  o=vec4(c,a)*uP.w;
}`
} as const

export type ProgramName = keyof typeof FS
