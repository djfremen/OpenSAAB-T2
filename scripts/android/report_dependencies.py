"""Resolve pinned AndroidX review dependencies for the existing SDK-only APK builds.

Gradle is the dependency resolver, not a second manually maintained Maven list.
The resulting AAR resources/R classes are also included in command-line test APKs.
"""
import json, os, shutil, subprocess, xml.etree.ElementTree as ET, zipfile
from pathlib import Path

class ReportDependencies:
    def __init__(self, repo, output, env):
        self.repo, self.output, self.env = repo, output, env
        manifest = output / 'report-dependencies.json'
        resolver_env = dict(env)
        java = os.environ.get('OPENSAAB_GRADLE_JAVA_HOME')
        if not java:
            java = subprocess.check_output(['/usr/libexec/java_home', '-v', '21'], text=True).strip()
        resolver_env['JAVA_HOME'] = java
        subprocess.run([str(repo/'gradlew'), ':app:exportReportDependencies',
                        '-PreportDependencyOutput='+str(manifest), '--console=plain'],
                       cwd=repo, env=resolver_env, check=True)
        artifacts = json.loads(manifest.read_text())
        self.jars, self.resources, self.packages = [], [], []
        extracted = output/'report-libraries';shutil.rmtree(extracted,ignore_errors=True);extracted.mkdir()
        for path in dict.fromkeys(a['path'] for a in artifacts):
            path=Path(path)
            if path.suffix=='.jar':self.jars.append(path);continue
            directory=extracted/path.stem;directory.mkdir()
            with zipfile.ZipFile(path) as aar:aar.extractall(directory)
            for jar in [directory/'classes.jar',*sorted((directory/'libs').glob('*.jar'))]:
                if jar.exists():self.jars.append(jar)
            if (directory/'res').is_dir():self.resources.append(directory/'res')
            package=ET.parse(directory/'AndroidManifest.xml').getroot().get('package')
            if package:self.packages.append(package)
        self.generated=output/'report-generated';shutil.rmtree(self.generated,ignore_errors=True);self.generated.mkdir()

    def package(self,bt,jar,manifest,resources,unsigned):
        compiled=[]
        for i,directory in enumerate([*self.resources,resources]):
            target=self.output/('report-res-'+str(i)+'.zip')
            subprocess.run([str(bt/'aapt2'),'compile','--dir',str(directory),'-o',str(target)],env=self.env,check=True)
            compiled.extend(['-R',str(target)])
        subprocess.run([str(bt/'aapt2'),'link','-o',str(unsigned),'--manifest',str(manifest),
                        '-I',str(jar),'--java',str(self.generated),'--extra-packages',':'.join(self.packages),
                        '--auto-add-overlay',*compiled],env=self.env,check=True)

    def compile(self,jdk,jar,classes,sources):
        cp=os.pathsep.join(map(str,[jar,*self.jars]))
        subprocess.run([str(jdk/'bin/javac'),'-source','8','-target','8','-cp',cp,'-d',str(classes),
                        *map(str,sources),*map(str,self.generated.rglob('*.java'))],env=self.env,check=True)

    def dex(self,bt,jar,classes):
        for stale in self.output.glob('classes*.dex'):stale.unlink()
        subprocess.run([str(bt/'d8'),'--min-api','26','--lib',str(jar),'--output',str(self.output),
                        *map(str,classes.rglob('*.class')),*map(str,self.jars)],env=self.env,check=True)
