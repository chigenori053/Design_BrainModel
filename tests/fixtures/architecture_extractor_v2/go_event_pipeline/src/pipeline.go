package pipeline

import "fmt"

func Stage1Ingest(input string) string {
    return fmt.Sprintf("ingest:%s", input)
}

func Stage2Transform(input string) string {
    return fmt.Sprintf("transform:%s", input)
}

func Stage3Publish(input string) string {
    return fmt.Sprintf("publish:%s", input)
}
