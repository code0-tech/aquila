import com.github.jengelman.gradle.plugins.shadow.tasks.ShadowJar
import com.google.protobuf.gradle.id

plugins {
    id("java")
    id("io.micronaut.aot") version "4.4.0"
    id("io.micronaut.application") version "4.4.0"
    id("io.micronaut.test-resources") version "4.4.0"
    id("com.google.protobuf") version "0.9.4"
    id("io.github.goooler.shadow") version "8.1.7"
}

group = "tech.code0"
version = "1.0-SNAPSHOT"

repositories {
    mavenCentral()
}

dependencies {
    annotationProcessor("io.micronaut:micronaut-http-validation")
    annotationProcessor("io.micronaut.serde:micronaut-serde-processor")
    implementation("com.oracle.coherence.ce:coherence")
    implementation("com.oracle.coherence.ce:coherence-java-client")
    implementation("io.micronaut.rabbitmq:micronaut-rabbitmq")
    implementation("io.micronaut.redis:micronaut-redis-lettuce")
    implementation("io.micronaut.serde:micronaut-serde-jackson")
    compileOnly("io.micronaut:micronaut-http-client")
    runtimeOnly("ch.qos.logback:logback-classic")
    testImplementation("io.micronaut:micronaut-http-client")

    implementation("javax.annotation:javax.annotation-api:1.3.2")
    implementation("com.google.protobuf:protobuf-java:4.27.2")

    implementation("com.gitlab.taucher2003.t2003-utils:log:1.1-beta.13")
    compileOnly("org.projectlombok:lombok:1.18.34")
    annotationProcessor("org.projectlombok:lombok:1.18.34")

    implementation("com.google.code.gson:gson:2.11.0")
    implementation("io.grpc:grpc-netty-shaded:1.65.0")
    implementation("io.grpc:grpc-protobuf:1.65.0")
    implementation("io.grpc:grpc-stub:1.65.0")

    testImplementation("io.micronaut.test:micronaut-test-junit5")
    testImplementation("org.junit.jupiter:junit-jupiter-api")
    testImplementation("org.mockito:mockito-junit-jupiter")
    testImplementation("io.grpc:grpc-testing:1.57.0")
    testRuntimeOnly("org.junit.jupiter:junit-jupiter-engine")
}


application {
    mainClass = "tech.code0.AquilaServer"
}
java {
    sourceCompatibility = JavaVersion.toVersion("21")
    targetCompatibility = JavaVersion.toVersion("21")
}

sourceSets {
    main {
        java {
            srcDirs("build/generated/source/proto/main/grpc")
            srcDirs("build/generated/source/proto/main/java")
        }
    }
}

protobuf {
    protoc {
        artifact = "com.google.protobuf:protoc:4.27.2"
    }

    plugins {
        id("grpc") {
            artifact = "io.grpc:protoc-gen-grpc-java:1.65.0"
        }
    }

    generateProtoTasks {
        all().forEach { task ->
            task.plugins {
                id("grpc")
            }
        }
    }
}

micronaut {
    runtime("netty")
    testRuntime("junit5")
    processing {
        incremental(true)
        annotations("tech.code0.*")
    }
    testResources {
        sharedServer = true
    }
    aot {
        optimizeServiceLoading = false
        convertYamlToJava = false
        precomputeOperations = true
        cacheEnvironment = true
        optimizeClassLoading = true
        deduceEnvironment = true
        optimizeNetty = true
        replaceLogbackXml = true
    }
}

tasks.named<ShadowJar>("shadowJar") {
    manifest {
        attributes(
            "Main-Class" to "tech.code0.AquilaServer"
        )
    }
}

tasks.named<io.micronaut.gradle.docker.NativeImageDockerfile>("dockerfileNative") {
    jdkVersion = "21"
}