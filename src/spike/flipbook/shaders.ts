// GLSL for the lit book scene and the baked desk, ported from mockup E. Scene colours and the
// light come in as uniforms (see applySceneUniforms below), so a skin changes them with no code change.
// Book size is baked in as constants because it is fixed per book (pad-to-uniform aspect).

import type { Program } from "./gl";
import type { Scene } from "./scene";

export type ShaderGeometry = { W: number; H: number; MRG: number; NV: number };

const f = (x: number) => x.toFixed(5);

const NOISE = `
float h21(vec2 p){vec3 p3=fract(vec3(p.xyx)*.1031);p3+=dot(p3,p3.yzx+33.33);return fract((p3.x+p3.y)*p3.z);}
vec2 h22(vec2 p){vec3 p3=fract(vec3(p.xyx)*vec3(.1031,.1030,.0973));p3+=dot(p3,p3.yzx+33.33);return fract((p3.xx+p3.yz)*p3.zy);}
float vnoise(vec2 p){vec2 i=floor(p),f=fract(p);f=f*f*(3.-2.*f);i=mod(i,289.);vec2 j=mod(i+1.,289.);
 return mix(mix(h21(i),h21(vec2(j.x,i.y)),f.x),mix(h21(vec2(i.x,j.y)),h21(j),f.x),f.y);}
float gnoise(vec2 p){vec2 i=floor(p),f=fract(p);vec2 u=f*f*f*(f*(f*6.-15.)+10.);i=mod(i,289.);vec2 j=mod(i+1.,289.);
 float a=dot(h22(i)*2.-1.,f),b=dot(h22(vec2(j.x,i.y))*2.-1.,f-vec2(1,0)),c=dot(h22(vec2(i.x,j.y))*2.-1.,f-vec2(0,1)),d=dot(h22(j)*2.-1.,f-1.);
 return mix(mix(a,b,u.x),mix(c,d,u.x),u.y);}
float fbm(vec2 p){float s=0.,a=.5;for(int i=0;i<5;i++){s+=a*gnoise(p);p=p*2.02+vec2(3.1,1.7);a*=.5;}return s;}
float sdBox(vec2 p,vec2 b){vec2 d=abs(p)-b;return length(max(d,0.))+min(max(d.x,d.y),0.);}
`;

export const VS_LIT = `#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;layout(location=1) in vec3 aNor;layout(location=2) in vec2 aUV;
uniform mat4 uVP,uModel;out vec3 vW;out vec3 vL;out vec3 vN;out vec2 vUV;
void main(){vec4 w=uModel*vec4(aPos,1.);vW=w.xyz;vL=aPos;vN=mat3(uModel)*aNor;vUV=aUV;gl_Position=uVP*w;}`;

export function fsLit(g: ShaderGeometry): string {
  const hb = `vec2(${f(g.W + g.MRG)},${f(g.H / 2 + g.MRG)})`;
  return `#version 300 es
precision highp float;
in vec3 vW;in vec3 vL;in vec3 vN;in vec2 vUV;
uniform int uMat,uLightType;uniform sampler2D uTexA;uniform sampler2D uTexB;
uniform vec3 uLightPos,uLightDir,uCam,uLightCol,uAmbient;uniform vec4 uFoot[${g.NV}];
uniform vec3 uLeatherDark,uLeatherLight,uGilt,uRibbon,uPageEdge;
uniform float uShadow,uFlag,uRibbonLen,uAlpha,uExposure,uVignette;uniform vec2 uRes,uTexel,uPool;uniform mat4 uBookInv;
out vec4 o;
${NOISE}
vec3 toLin(vec3 c){return pow(max(c,0.),vec3(2.2));}
vec3 envMap(vec3 R,vec3 L){vec3 c=mix(uAmbient*.5,uLightCol*.17,smoothstep(-.1,.95,R.z));
 c+=uLightCol*.85*pow(max(dot(R,L),0.),7.)*.9;c+=uLightCol*.09*smoothstep(.4,1.,R.y*.5+R.z*.6);return c;}
// a point light (lamp) throws an elliptical pool on the desk; a directional light is even
float lampPool(vec2 p){if(uLightType!=0)return 1.;vec2 q=(p-uPool)*vec2(.78,1.);float k=1.-smoothstep(.22,2.15,length(q));k=k*k*(3.-2.*k);return .1+.9*k;}
float leafShadow(vec3 p){if(uShadow<.5)return 1.;
 float fy=clamp((${f(g.H / 2)}-p.y)/${f(g.H)},0.,1.)*${(g.NV - 1).toFixed(1)};int i0=int(floor(fy));int i1=min(i0+1,${g.NV - 1});
 vec4 ft=mix(uFoot[i0],uFoot[i1],fy-float(i0));float h=ft.z;float mn=ft.x+.04*h,mx=ft.y+.22*h;
 float d=p.x>mx?(p.x-mx)/(.02+.42*h):(p.x<mn?(mn-p.x)/(.01+.06*h):0.);
 float s=exp(-d*1.4);float str=smoothstep(0.,.045,h)*mix(.72,.48,clamp(h/.9,0.,1.));return 1.-str*s;}
void main(){
 vec3 N=normalize(vN);vec3 V=normalize(uCam-vW);vec3 L;float att;
 if(uLightType==0){vec3 Lv=uLightPos-vW;float ld2=dot(Lv,Lv);L=Lv*inversesqrt(ld2);att=2.75/(1.+.1*ld2)*lampPool(vW.xy);}
 else{L=normalize(-uLightDir);att=1.;}
 vec3 LC=uLightCol*att;vec3 AMB=uAmbient*(.55+.45*lampPool(vW.xy));vec3 col=vec3(0);vec3 H2=normalize(L+V);
 if(uMat==9){float a=uAlpha*smoothstep(1.,.1,abs(vUV.y));if(uRibbonLen-vUV.x<.05*(1.-abs(vUV.y)))discard;o=vec4(0,0,0,a);return;}
 if(uMat==7){ // desk: baked procedural wood, lit per frame
  vec2 sc=gl_FragCoord.xy/uRes;vec4 c0=texture(uTexA,sc);
  float hl=texture(uTexA,sc-vec2(uTexel.x,0)).a,hr=texture(uTexA,sc+vec2(uTexel.x,0)).a,hd=texture(uTexA,sc-vec2(0,uTexel.y)).a,hu=texture(uTexA,sc+vec2(0,uTexel.y)).a;
  vec3 Nt=normalize(vec3((hl-hr)*1.1,(hd-hu)*1.1,1.));vec3 alb=toLin(c0.rgb);
  vec2 pb=(uBookInv*vec4(vW,1.)).xy;vec2 hb=${hb};
  float sd=sdBox(pb,hb)-.004;float sh=mix(.14,1.,smoothstep(-.004,.03,sd));
  sh*=mix(.42,1.,smoothstep(-.06,.46,sdBox(pb-vec2(.07,-.08),hb)));
  float diff=max(dot(Nt,L),0.);float nh=max(dot(Nt,H2),0.);
  float spec=pow(nh,36.)*.10+pow(nh,7.)*.022;
  col=alb*(AMB+LC*diff)*sh+LC*spec*sh*smoothstep(.25,.5,c0.a);
 }else if(uMat==0){ // scanned page; texture holds the scan letterboxed onto paper
  bool bk=uFlag>.5&&!gl_FrontFacing;
  vec4 ta=texture(uTexA,vUV),tb=texture(uTexB,vec2(1.-vUV.x,vUV.y));vec4 tx=bk?tb:ta;if(bk)N=-N;
  vec3 alb=toLin(tx.rgb);float ndl=dot(N,L);float wrap=max((ndl+.3)/1.3,0.);
  float spec=pow(max(dot(N,H2),0.),18.)*.045;col=alb*(AMB+LC*wrap)+LC*spec;
  if(uFlag>.5){float back=max(-dot(N,L),0.);col+=alb*LC*back*.3*vec3(1.,.8,.5);}
  float sdist=uFlag>.5?vUV.x:abs(vL.x);col*=mix(uFlag>.5?.78:.58,1.,smoothstep(0.,.17,sdist));
  if(uFlag<.5)col*=leafShadow(vL);
 }else if(uMat==1){ // page-block edges
  float lay=vUV.y,fr=fract(lay);float line=smoothstep(0.,.3,fr)*smoothstep(1.,.6,fr);float n=vnoise(vec2(vUV.x*60.,lay*.7));
  vec3 base=uPageEdge*(.7+.3*line)*(.86+.14*n);col=toLin(base)*(AMB*1.4+LC*max((dot(N,L)+.45)/1.45,0.));col*=leafShadow(vL);
 }else if(uMat==2){ // tooled leather board
  vec2 lp=vL.xy;float g1=vnoise(lp*260.),g2=vnoise(lp*95.+7.),g3=fbm(lp*3.)*.5+.5;
  vec3 base=mix(uLeatherDark,uLeatherLight,clamp(.35*g2+.25*g1+.6*g3-.05,0.,1.));
  float edge=1.-N.z;base=mix(base,base*1.7+vec3(.03,.02,.01),smoothstep(.1,.6,edge)*.45*(.5+g2));
  vec2 hb=${hb};vec2 dd=hb-abs(lp);float rim=min(dd.x,dd.y);
  float fil=(1.-smoothstep(.0011,.0024,abs(rim-.017)))*step(.95,N.z);
  float aoB=mix(.3,1.,smoothstep(0.,.035,sdBox(lp,vec2(${f(g.W - 0.01)},${f(g.H / 2)}))));
  vec3 alb=toLin(base);float ndl=max(dot(N,L),0.);float sp=pow(max(dot(N,H2),0.),26.)*.16*(.6+g1*.8);
  col=alb*(AMB+LC*ndl)+LC*sp;vec3 Rr=reflect(-V,N);
  vec3 gc=toLin(uGilt)*(.3*LC*ndl+1.4*envMap(Rr,L))+LC*pow(max(dot(N,H2),0.),50.)*.7;
  col=mix(col,gc,fil*(.55+.45*g1));if(N.z>.95)col*=aoB;
 }else if(uMat==3){ // silk ribbon
  if(uRibbonLen-vUV.x<.06*(1.-abs(vUV.y)))discard;if(dot(N,V)<0.)N=-N;
  float weave=.84+.16*vnoise(vec2(vUV.y*7.,vUV.x*1500.));
  vec3 alb=toLin(uRibbon)*weave;float ndl=max((dot(N,L)+.25)/1.25,0.);float nh=max(dot(N,H2),0.);
  col=alb*(AMB+LC*ndl)+LC*(pow(nh,24.)*.28+pow(nh,5.)*.04)*vec3(1.,.6,.55);col*=.74+.26*smoothstep(1.,.7,abs(vUV.y));
 }
 vec2 vq=gl_FragCoord.xy/uRes-vec2(.45,.57);vq.x*=uRes.x/uRes.y;float vig=1.-smoothstep(.34,1.12,length(vq*vec2(.8,1.)));col*=mix(1.-uVignette,1.,vig);
 col=1.-exp(-col*uExposure);col=pow(col,vec3(1./2.2));col+=(h21(gl_FragCoord.xy)-.5)/255.;
 o=vec4(col,1.);
}`;
}

export const VS_FULL = `#version 300 es
layout(location=0) in vec2 aP;void main(){gl_Position=vec4(aP,0.,1.);}`;

// Procedural wood planks baked once per camera into a screen-sized texture. RGB is sRGB albedo,
// A is a height used for the per-frame bump.
export const FS_BAKE = `#version 300 es
precision highp float;
uniform mat4 uInvVP;uniform vec2 uRes;uniform vec3 uDeskLight,uDeskMid,uDeskDark;out vec4 o;
${NOISE}
vec2 groundAt(vec2 fc){vec2 nd=fc/uRes*2.-1.;vec4 a=uInvVP*vec4(nd,-1.,1.),b=uInvVP*vec4(nd,1.,1.);a/=a.w;b/=b.w;float t=-a.z/(b.z-a.z);return mix(a.xyz,b.xyz,t).xy;}
float fb(vec2 p){float s=0.,a=.5;mat2 m=mat2(1.6,1.2,-1.2,1.6);for(int i=0;i<5;i++){s+=a*vnoise(p);p=m*p+17.1;a*=.5;}return s;}
float hh(vec2 p){return h21(p*1.731+.5);}
vec4 wood(vec2 p){
 const float PWD=.84;float yy=p.y+.41;float pid=floor(yy/PWD),py=yy-pid*PWD;
 float r1=hh(vec2(pid,1.37)),r2=hh(vec2(pid,7.71)),r3=hh(vec2(pid,4.13));
 float Lb=3.6+2.4*r1;float xx=p.x+r2*9.+4.;float jid=floor(xx/Lb),jx=xx-jid*Lb;
 float sd=pid*17.+jid*3.+.5;float s1=hh(vec2(sd,2.3)),s2=hh(vec2(sd,5.9)),s3=hh(vec2(sd,9.4));
 vec2 b=vec2(jx-Lb*.5+(s1-.5)*1.4,py-PWD*.5);
 float w1=fb(vec2(b.x*.32+s2*31.,b.y*1.3+s3*17.));float w2=fb(vec2(b.x*1.1+11.+s1*9.,b.y*4.2));
 float cy=(s3-.5)*.9+.25*sign(s3-.5);
 float rr=length(vec2(b.x*(.09+.08*s2),b.y-cy+.30*(w1-.5)));
 float ring=rr*26.+w1*3.+w2*.8+2.6*fb(vec2(b.x*.12+s1*7.,b.y*5.));float t=fract(ring);
 float late=smoothstep(.50,.95,t)-smoothstep(.95,1.,t);
 float streak=fb(vec2(b.x*1.7+s1*50.,b.y*52.+w1*6.));float fine=vnoise(vec2(b.x*5.,b.y*360.+w2*20.));
 float pore=smoothstep(.80,.97,vnoise(vec2(b.x*60.,b.y*900.+w1*40.)))*(.35+.65*late);
 float figure=fb(vec2(b.x*.6+s2*20.,b.y*2.2+3.));float band=fb(vec2(b.x*.5+s2*13.,b.y*13.+w1*3.));
 float hair=fb(vec2(b.x*2.6+s3*40.,b.y*170.+w2*12.));
 late*=.35+.9*fb(vec2(b.x*.9+s1*5.,b.y*3.+9.));
 float d=late*.42+(streak-.5)*.6+(band-.5)*.75+(hair-.5)*.55+(fine-.5)*.2+(figure-.5)*.6+(r3-.5)*.3;
 vec3 col=mix(uDeskLight,uDeskMid,smoothstep(-.15,.35,d));col=mix(col,uDeskDark,smoothstep(.28,.98,d));col=mix(col,uDeskDark*.8,pore*.35);
 col*=mix(vec3(1.),vec3(1.04,.97,.93),s2);
 float seamD=min(min(py,PWD-py),min(jx,Lb-jx));col*=1.-.7*(1.-smoothstep(0.,.0035,seamD));
 float h=-late*.1-pore*.35+(streak-.5)*.05+fine*.03-(1.-smoothstep(0.,.006,seamD))*1.4;
 return vec4(col,clamp(.62+h*.42,0.,1.));}
void main(){vec4 a=vec4(0);
 a+=wood(groundAt(gl_FragCoord.xy+vec2(-.3,-.1)));a+=wood(groundAt(gl_FragCoord.xy+vec2(.1,-.3)));
 a+=wood(groundAt(gl_FragCoord.xy+vec2(.3,.1)));a+=wood(groundAt(gl_FragCoord.xy+vec2(-.1,.3)));o=a*.25;}`;

// Uploads the skin's light and material values. Every key comes from scene.json.
export function applySceneUniforms(gl: WebGL2RenderingContext, u: Program["u"], sc: Scene) {
  const l = sc.light, m = sc.material;
  gl.uniform1i(u.uLightType, l.type === "point" ? 0 : 1);
  gl.uniform3fv(u.uLightCol, l.color);
  if (l.type === "point") {
    gl.uniform3fv(u.uLightPos, l.position);
    gl.uniform2fv(u.uPool, l.poolCenter);
  } else {
    gl.uniform3fv(u.uLightDir, l.direction);
  }
  gl.uniform3fv(u.uAmbient, sc.ambient);
  gl.uniform1f(u.uExposure, sc.exposure);
  gl.uniform1f(u.uVignette, sc.vignette);
  gl.uniform3fv(u.uLeatherDark, m.coverLeather.dark);
  gl.uniform3fv(u.uLeatherLight, m.coverLeather.light);
  gl.uniform3fv(u.uGilt, m.gilt);
  gl.uniform3fv(u.uRibbon, m.ribbon);
  gl.uniform3fv(u.uPageEdge, m.pageEdge);
}
